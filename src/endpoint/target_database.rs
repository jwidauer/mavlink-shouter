use parking_lot::{RwLock, RwLockUpgradableReadGuard as ReadGuard};
use std::net::SocketAddr;

use crate::mavlink;

pub struct TargetDatabase {
    targets: RwLock<Vec<(mavlink::SysCompId, SocketAddr)>>,
}

impl TargetDatabase {
    pub fn new() -> Self {
        Self {
            targets: RwLock::new(Vec::new()),
        }
    }

    pub fn insert_or_update(&self, sender: mavlink::SysCompId, addr: SocketAddr) {
        let targets = self.targets.upgradable_read();
        match targets.iter().position(|(t, _)| t == &sender) {
            Some(index) if targets[index].1 != addr => {
                let mut targets = ReadGuard::upgrade(targets);
                targets[index] = (sender, addr);
            }
            None => {
                let mut targets = ReadGuard::upgrade(targets);
                targets.push((sender, addr));
            }
            _ => {}
        }
    }

    pub fn get_target_addresses(&self, routing_info: &mavlink::RoutingInfo) -> Vec<SocketAddr> {
        self.targets
            .read()
            .iter()
            .filter(|(t, _)| routing_info.matches(*t))
            .map(|(_, addr)| *addr)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_or_update() -> Result<(), std::net::AddrParseError> {
        let db = TargetDatabase::new();
        let sender = mavlink::SysCompId::from((1, 1));
        let target = mavlink::SysCompId::from((1, 2));
        let routing_info = mavlink::RoutingInfo { sender, target };

        let addr = "127.0.0.1:14550".parse()?;
        db.insert_or_update(target, addr);
        assert_eq!(db.get_target_addresses(&routing_info), vec![addr]);
        Ok(())
    }

    #[test]
    fn test_insert_or_update_updates() -> Result<(), std::net::AddrParseError> {
        let db = TargetDatabase::new();
        let sender = mavlink::SysCompId::from((1, 1));
        let target = mavlink::SysCompId::from((1, 2));
        let routing_info = mavlink::RoutingInfo { sender, target };

        let addr = "127.0.0.1:14550".parse()?;
        db.insert_or_update(target, addr);
        let addr = "127.0.0.1:14551".parse()?;
        db.insert_or_update(target, addr);
        assert_eq!(db.get_target_addresses(&routing_info), vec![addr]);
        Ok(())
    }

    #[test]
    fn test_insert_or_update_does_nothing_when_inserting_twice(
    ) -> Result<(), std::net::AddrParseError> {
        let db = TargetDatabase::new();
        let sender = mavlink::SysCompId::from((1, 1));
        let target = mavlink::SysCompId::from((1, 2));
        let routing_info = mavlink::RoutingInfo { sender, target };

        let addr = "127.0.0.1:14550".parse()?;
        db.insert_or_update(target, addr);
        db.insert_or_update(target, addr);
        assert_eq!(db.get_target_addresses(&routing_info), vec![addr]);
        Ok(())
    }

    #[test]
    fn test_get_matching_returns_empty_when_target_not_found(
    ) -> Result<(), std::net::AddrParseError> {
        let db = TargetDatabase::new();

        let target = mavlink::SysCompId::from((1, 1));
        let addr1 = "127.0.0.1:14550".parse()?;
        db.insert_or_update(target, addr1);

        let sender = mavlink::SysCompId::from((1, 1));
        let target = mavlink::SysCompId::from((1, 2));
        let routing_info = mavlink::RoutingInfo { sender, target };
        assert_eq!(db.get_target_addresses(&routing_info), Vec::new());
        Ok(())
    }

    #[test]
    fn test_get_matching_returns_matching_targets() -> Result<(), std::net::AddrParseError> {
        let db = TargetDatabase::new();

        let target = mavlink::SysCompId::from((1, 1));
        let addr1 = "127.0.0.1:14550".parse()?;
        db.insert_or_update(target, addr1);

        let target = mavlink::SysCompId::from((1, 2));
        let addr2 = "127.0.0.1:14551".parse()?;
        db.insert_or_update(target, addr2);

        let target = mavlink::SysCompId::from((2, 1));
        let addr3 = "127.0.0.1:14552".parse()?;
        db.insert_or_update(target, addr3);

        let sender = mavlink::SysCompId::from((2, 1));
        let target = mavlink::SysCompId::from((1, 0));
        let routing_info = mavlink::RoutingInfo { sender, target };
        assert_eq!(db.get_target_addresses(&routing_info), vec![addr1, addr2]);
        Ok(())
    }
}
