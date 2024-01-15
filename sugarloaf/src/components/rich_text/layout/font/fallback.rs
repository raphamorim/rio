use super::types::FamilyId;

#[derive(Copy, Clone)]
pub struct Fallbacks {
    entries: [FamilyId; 6],
}

impl Fallbacks {
    pub fn new() -> Self {
        Self {
            entries: [FamilyId(0); 6],
        }
    }

    pub fn len(&self) -> usize {
        self.entries[5].to_usize()
    }

    pub fn push(&mut self, family: FamilyId) -> bool {
        let len = self.entries[5].to_usize();
        if len >= 5 {
            return false;
        }
        self.entries[len] = family;
        self.entries[5].0 += 1;
        true
    }

    pub fn get(&self) -> &[FamilyId] {
        let len = self.entries[5].to_usize();
        &self.entries[..len]
    }
}
