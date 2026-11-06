// VpnCloud - Peer-to-Peer VPN
// Copyright (C) 2015-2021  Dennis Schwerdel
// This software is licensed under GPL-3 or newer (see LICENSE.md)

use std::{
    cmp::min, collections::HashMap, io, io::Write, marker::PhantomData, net::SocketAddr,
};

use crate::{
    types::{Address, Range, RangeList},
    util::{addr_nice, Duration, Time, TimeSource},
};

type Hash = ahash::RandomState;

struct CacheValue {
    peer: SocketAddr,
    timeout: Time,
}

struct ClaimEntry {
    peer: SocketAddr,
    claim: Range,
    timeout: Time,
}

pub struct ClaimTable<TS: TimeSource> {
    cache: HashMap<Address, CacheValue, Hash>,
    cache_timeout: Duration,
    claims: Vec<ClaimEntry>,
    claim_timeout: Duration,
    _dummy: PhantomData<TS>,
}

impl<TS: TimeSource> ClaimTable<TS> {
    pub fn new(cache_timeout: Duration, claim_timeout: Duration) -> Self {
        Self { cache: HashMap::default(), cache_timeout, claims: vec![], claim_timeout, _dummy: PhantomData }
    }

    pub fn cache(&mut self, addr: Address, peer: SocketAddr) {
        // HOT PATH
        self.cache.insert(addr, CacheValue { peer, timeout: TS::now() + self.cache_timeout as Time });
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear()
    }

    pub fn set_claims(&mut self, peer: SocketAddr, mut claims: RangeList) {
        let mut removed_claim = false;
        for entry in &mut self.claims {
            if entry.peer == peer {
                let pos = claims.iter().position(|r| r == &entry.claim);
                if let Some(pos) = pos {
                    entry.timeout = TS::now() + self.claim_timeout as Time;
                    claims.swap_remove(pos);
                    if claims.is_empty() {
                        break;
                    }
                } else {
                    entry.timeout = 0;
                    removed_claim = true;
                }
            }
        }
        for claim in claims {
            self.claims.push(ClaimEntry { peer, claim, timeout: TS::now() + self.claim_timeout as Time })
        }
        if removed_claim {
            for entry in self.cache.values_mut() {
                if entry.peer == peer {
                    entry.timeout = 0
                }
            }
        }
        self.housekeep()
    }

    pub fn remove_claims(&mut self, peer: SocketAddr) {
        for entry in &mut self.claims {
            if entry.peer == peer {
                entry.timeout = 0
            }
        }
        for entry in self.cache.values_mut() {
            if entry.peer == peer {
                entry.timeout = 0
            }
        }
        self.housekeep()
    }

    pub fn lookup(&mut self, addr: Address) -> Option<SocketAddr> {
        // HOT PATH
        if let Some(entry) = self.cache.get(&addr) {
            return Some(entry.peer);
        }
        // COLD PATH
        let mut found = None;
        let mut prefix_len = -1;
        for entry in &self.claims {
            if entry.claim.prefix_len as isize > prefix_len && entry.claim.matches(addr) {
                found = Some(entry);
                prefix_len = entry.claim.prefix_len as isize;
            }
        }
        if let Some(entry) = found {
            self.cache.insert(
                addr,
                CacheValue { peer: entry.peer, timeout: min(TS::now() + self.cache_timeout as Time, entry.timeout) },
            );
            return Some(entry.peer);
        }
        None
    }

    pub fn housekeep(&mut self) {
        let now = TS::now();
        self.cache.retain(|_, v| v.timeout >= now);
        self.claims.retain(|e| e.timeout >= now);
    }

    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    pub fn claim_len(&self) -> usize {
        self.claims.len()
    }

    /// Write out the table
    pub fn write_out<W: Write>(&self, out: &mut W) -> Result<(), io::Error> {
        let now = TS::now();
        writeln!(out, "forwarding_table:")?;
        writeln!(out, "  claims:")?;
        for entry in &self.claims {
            writeln!(
                out,
                "    - \"{}\": {{ peer: \"{}\", timeout: {} }}",
                entry.claim,
                addr_nice(entry.peer),
                entry.timeout - now
            )?;
        }
        writeln!(out, "  cache:")?;
        for (addr, entry) in &self.cache {
            writeln!(
                out,
                "    - \"{}\": {{ peer: \"{}\", timeout: {} }}",
                addr,
                addr_nice(entry.peer),
                entry.timeout - now
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{types::Address, util::MockTimeSource};
    use std::{net::SocketAddr, str::FromStr};

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    fn range(s: &str) -> Range {
        Range::from_str(s).unwrap()
    }

    #[test]
    fn longest_prefix_wins() {
        MockTimeSource::set_time(1000);
        let peer1 = addr("1.1.1.1:1");
        let peer2 = addr("2.2.2.2:2");
        let mut table = ClaimTable::<MockTimeSource>::new(300, 300);
        table.set_claims(peer1, smallvec::smallvec![range("10.0.0.0/8")]);
        table.set_claims(peer2, smallvec::smallvec![range("10.1.0.0/16")]);
        // More specific claim wins regardless of insertion order
        assert_eq!(Some(peer2), table.lookup(Address::from_str("10.1.2.3").unwrap()));
        assert_eq!(Some(peer1), table.lookup(Address::from_str("10.2.3.4").unwrap()));
        assert_eq!(None, table.lookup(Address::from_str("11.0.0.1").unwrap()));
    }

    #[test]
    fn lookup_caches_and_cache_expires() {
        MockTimeSource::set_time(1000);
        let peer = addr("1.1.1.1:1");
        let mut table = ClaimTable::<MockTimeSource>::new(60, 300);
        table.set_claims(peer, smallvec::smallvec![range("10.0.0.0/8")]);
        let dst = Address::from_str("10.1.2.3").unwrap();
        assert_eq!(Some(peer), table.lookup(dst));
        assert_eq!(1, table.cache_len());
        // Stale cache entries are still served until housekeep runs
        MockTimeSource::set_time(1100);
        assert_eq!(Some(peer), table.lookup(dst));
        table.housekeep();
        assert_eq!(0, table.cache_len());
    }

    #[test]
    fn cache_entry_dies_with_claim() {
        MockTimeSource::set_time(1000);
        let peer = addr("1.1.1.1:1");
        // Cache timeout longer than claim timeout: the cached entry must
        // not outlive the claim it resolved through
        let mut table = ClaimTable::<MockTimeSource>::new(300, 60);
        table.set_claims(peer, smallvec::smallvec![range("10.0.0.0/8")]);
        let dst = Address::from_str("10.1.2.3").unwrap();
        assert_eq!(Some(peer), table.lookup(dst));
        MockTimeSource::set_time(1100);
        table.housekeep();
        assert_eq!(0, table.claim_len());
        assert_eq!(None, table.lookup(dst));
    }

    #[test]
    fn set_claims_replaces_and_invalidates_cache() {
        MockTimeSource::set_time(1000);
        let peer = addr("1.1.1.1:1");
        let mut table = ClaimTable::<MockTimeSource>::new(300, 300);
        let dst = Address::from_str("10.1.2.3").unwrap();
        table.set_claims(peer, smallvec::smallvec![range("10.0.0.0/8")]);
        assert_eq!(Some(peer), table.lookup(dst));
        // Narrowing claims must drop the old claim AND its cache entry
        table.set_claims(peer, smallvec::smallvec![range("10.1.0.0/16")]);
        assert_eq!(Some(peer), table.lookup(dst));
        table.set_claims(peer, smallvec::smallvec![]);
        assert_eq!(0, table.claim_len());
        assert_eq!(None, table.lookup(dst));
    }

    #[test]
    fn remove_claims_clears_everything() {
        MockTimeSource::set_time(1000);
        let peer = addr("1.1.1.1:1");
        let mut table = ClaimTable::<MockTimeSource>::new(300, 300);
        table.set_claims(peer, smallvec::smallvec![range("10.0.0.0/8")]);
        let dst = Address::from_str("10.1.2.3").unwrap();
        table.lookup(dst);
        table.cache(Address::from_str("10.9.9.9").unwrap(), peer);
        table.remove_claims(peer);
        assert_eq!(0, table.claim_len());
        assert_eq!(0, table.cache_len());
        assert_eq!(None, table.lookup(dst));
    }

    #[test]
    fn claims_expire_after_timeout() {
        MockTimeSource::set_time(1000);
        let peer = addr("1.1.1.1:1");
        let mut table = ClaimTable::<MockTimeSource>::new(300, 60);
        table.set_claims(peer, smallvec::smallvec![range("10.0.0.0/8")]);
        // Refreshing the same claim extends its lifetime
        MockTimeSource::set_time(1050);
        table.set_claims(peer, smallvec::smallvec![range("10.0.0.0/8")]);
        MockTimeSource::set_time(1100);
        table.housekeep();
        assert_eq!(1, table.claim_len());
        MockTimeSource::set_time(1200);
        table.housekeep();
        assert_eq!(0, table.claim_len());
    }

    #[test]
    fn write_out_format() {
        MockTimeSource::set_time(1000);
        let peer = addr("1.1.1.1:1");
        let mut table = ClaimTable::<MockTimeSource>::new(300, 300);
        table.set_claims(peer, smallvec::smallvec![range("10.0.0.0/8")]);
        table.lookup(Address::from_str("10.1.2.3").unwrap());
        let mut out = Vec::new();
        table.write_out(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("forwarding_table:"));
        assert!(text.contains("- \"10.0.0.0/8\": { peer: \"1.1.1.1:1\", timeout: 300 }"));
        assert!(text.contains("- \"10.1.2.3\": { peer: \"1.1.1.1:1\", timeout: 300 }"));
    }
}
