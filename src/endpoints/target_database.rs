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
