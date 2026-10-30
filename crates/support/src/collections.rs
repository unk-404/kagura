pub type FastHashMap<K, V> = hashbrown::HashMap<K, V, foldhash::fast::RandomState>;
pub type FastHashSet<T> = hashbrown::HashSet<T, foldhash::fast::RandomState>;
