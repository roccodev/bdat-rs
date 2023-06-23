/// A simple hash table with separate chaining.
/// When the table is written, chain nodes are linked together in column info tables.
pub(super) struct HashTable {
    slots: Vec<Vec<u16>>,
    hash_mod: u32,
}

impl HashTable {
    pub fn new(hash_mod: u32) -> Self {
        let mut table = Self {
            slots: Vec::new(),
            hash_mod: 0,
        };
        table.grow(hash_mod);
        table
    }

    /// If the key was already present in the table, behavior is undefined.
    pub fn insert_unique(&mut self, key: &str, value: u16) {
        let idx = self.hash(key.as_ref()) as usize;
        self.slots[idx].push(value);
    }

    fn grow(&mut self, new_mod: u32) {
        self.hash_mod = new_mod;
        self.slots = vec![Vec::new(); self.hash_mod as usize];
    }

    fn hash(&self, text: &str) -> u32 {
        if text.is_empty() {
            return 0;
        }
        let first = text.chars().next().unwrap() as u32;
        let sum = text
            .bytes()
            .skip(1)
            .take(7)
            .fold(first, |old, ch| old.wrapping_mul(7).wrapping_add(ch as u32));
        sum % self.hash_mod
    }

    #[cfg(test)]
    fn get_slot(&self, val: u16) -> Option<usize> {
        self.slots.iter().position(|v| v.contains(&val))
    }
}

#[cfg(test)]
mod tests {
    use super::HashTable;

    #[test]
    fn test_table_mod_61() {
        let mut table = HashTable::new(61);
        table.insert_unique("name", 100);
        table.insert_unique("style", 200);

        assert_eq!(37, table.get_slot(100).unwrap());
        assert_eq!(60, table.get_slot(200).unwrap());

        table.insert_unique("KizunaReward1", 300);
        table.insert_unique("KizunaReward2", 400);

        assert_eq!(9, table.get_slot(300).unwrap());
        assert_eq!(9, table.get_slot(400).unwrap());
    }

    #[test]
    fn test_hash_mod_61() {
        let table = HashTable::new(61);
        assert_eq!(37, table.hash("name"));
        assert_eq!(60, table.hash("style"));
    }
}
