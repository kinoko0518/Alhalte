mod events;

use std::{collections::HashMap, mem};

use crate::UserID;

use canner::{Blob, BlobID, encrypted::Encrypted};
pub use events::{Events, Read, Write};

type SceneID = u32;

struct Head {
    cindex: u32,
    index: u16,
}

struct Biblio(Vec<BlobID>);

impl Biblio {
    pub fn is_current(&self, head: Head) -> bool {
        self.0.len() == head.cindex as usize
    }
}

struct Scene {
    // log
    hotchunk: Vec<Option<Encrypted>>,
    biblio: Biblio,
    head: Head,
    // state
    state: HashMap<String, Blob>,
}

enum SceneError {
    EditFailed,
}

impl Scene {
    pub fn append(&mut self, tin: Encrypted) {
        self.hotchunk.push(Some(tin));
    }

    pub fn edit(&mut self, tin: Encrypted, head: Head) -> Result<(), SceneError> {
        if self.biblio.is_current(head) {
            self.hotchunk
                .get_mut(head.cindex as usize)
                .map(|slot| mem::replace(slot, Some(tin)))
                .flatten()
                .ok_or(SceneError::EditFailed)
        } else {
            let scene_id = self
                .biblio
                .0
                .get(head.cindex as usize)
                .cloned()
                .ok_or(SceneError::EditFailed)?;
        }
    }
}

#[derive(Clone, Copy)]
enum Permission {
    Deny,
    Spectator,
    Player,
    GameMaster,
}

struct Banquette {
    default_permission: Permission,
    permissions: HashMap<UserID, Permission>,
    scenes: HashMap<SceneID, Scene>,
}

impl Banquette {
    pub fn get_permission(&self, user: UserID) -> Permission {
        self.permissions
            .get(&user)
            .cloned()
            .unwrap_or(self.default_permission)
    }
}
