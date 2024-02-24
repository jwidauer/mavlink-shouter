use parking_lot::{RwLock, RwLockUpgradableReadGuard as ReadGuard};
use std::{collections::HashMap, net::SocketAddr};

use crate::mavlink;

pub struct TargetDatabase {
    targets: RwLock<HashMap<mavlink::SysCompId, SocketAddr>>,
}

impl TargetDatabase {
    pub fn new() -> Self {
        Self {
            targets: RwLock::new(HashMap::new()),
        }
    }

    pub fn insert_or_update(&self, target: mavlink::SysCompId, addr: SocketAddr) {
        let targets = self.targets.upgradable_read();
        if !targets.contains_key(&target) || targets[&target] != addr {
            let mut targets = ReadGuard::upgrade(targets);
            targets.insert(target, addr);
        }
    }

    pub fn get(&self, target: &mavlink::SysCompId) -> Option<SocketAddr> {
        self.targets.read().get(target).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_or_update() -> Result<(), std::net::AddrParseError> {
        let db = TargetDatabase::new();
        let target = mavlink::SysCompId::from((1, 1));
        let addr = "127.0.0.1:14550".parse()?;
        db.insert_or_update(target, addr);
        assert_eq!(db.get(&target), Some(addr));
        Ok(())
    }

    #[test]
    fn test_insert_or_update_updates() -> Result<(), std::net::AddrParseError> {
        let db = TargetDatabase::new();
        let target = mavlink::SysCompId::from((1, 1));
        let addr = "127.0.0.1:14550".parse()?;
        db.insert_or_update(target, addr);
        let addr = "127.0.0.1:14551".parse()?;
        db.insert_or_update(target, addr);
        assert_eq!(db.get(&target), Some(addr));
        Ok(())
    }

    #[test]
    fn test_insert_or_update_does_nothing_when_inserting_twice(
    ) -> Result<(), std::net::AddrParseError> {
        let db = TargetDatabase::new();
        let target = mavlink::SysCompId::from((1, 1));
        let addr = "127.0.0.1:14550".parse()?;
        db.insert_or_update(target, addr);
        db.insert_or_update(target, addr);
        assert_eq!(db.get(&target), Some(addr));
        Ok(())
    }

    #[test]
    fn test_get_returns_none_when_target_not_found() {
        let db = TargetDatabase::new();
        let target = mavlink::SysCompId::from((1, 1));
        assert_eq!(db.get(&target), None);
    }
}
