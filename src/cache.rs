// Fork of https://github.com/maidsafe/lru_time_cache to be memory limited instead.
//
// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under the MIT license <LICENSE-MIT
// http://opensource.org/licenses/MIT> or the Modified BSD license <LICENSE-BSD
// https://opensource.org/licenses/BSD-3-Clause>, at your option. This file may not be copied,
// modified, or distributed except according to those terms. Please review the Licences for the
// specific language governing permissions and limitations relating to use of the SAFE Network
// Software.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maidsafe/QA/master/Images/maidsafe_logo.png",
    html_favicon_url = "https://maidsafe.net/img/favicon.ico",
    test(attr(forbid(warnings)))
)]
// For explanation of lint checks, run `rustc -W help` or see
// https://github.com/maidsafe/QA/blob/master/Documentation/Rust%20Lint%20Checks.md
#![forbid(
    bad_style,
    exceeding_bitshifts,
    mutable_transmutes,
    no_mangle_const_items,
    unknown_crate_types
)]
#![deny(
    deprecated,
    improper_ctypes,
    missing_docs,
    non_shorthand_field_patterns,
    overflowing_literals,
    plugin_as_library,
    stable_features,
    unconditional_recursion,
    unknown_lints,
    unsafe_code,
    unused_allocation,
    unused_attributes,
    unused_comparisons,
    unused_features,
    unused_parens,
    while_true
)]
#![warn(
    trivial_casts,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_results
)]
#![allow(
    box_pointers,
    missing_copy_implementations,
    missing_debug_implementations,
    variant_size_differences,
    dead_code
)]

#[cfg(feature = "fake_clock")]
extern crate fake_clock;
#[cfg(test)]
extern crate rand;

#[cfg(feature = "fake_clock")]
use fake_clock::FakeClock as Instant;
use std::borrow::Borrow;
use std::collections::{btree_map, BTreeMap, VecDeque};
#[cfg(not(feature = "fake_clock"))]
use std::time::Instant;
use std::usize;

/// An iterator over an `LruCache`'s entries that updates the timestamps as values are traversed.
pub struct Iter<'a, Key: 'a, Value: 'a> {
    map_iter_mut: btree_map::IterMut<'a, Key, (Value, Instant, usize)>,
    list: &'a mut VecDeque<Key>,
}

impl<'a, Key, Value> Iterator for Iter<'a, Key, Value>
where
    Key: Ord + Clone,
{
    type Item = (&'a Key, &'a Value);

    fn next(&mut self) -> Option<(&'a Key, &'a Value)> {
        let now = Instant::now();
        let not_expired = self
            .map_iter_mut
            .find(|&(_, &mut (_, instant, _))| instant > now);

        not_expired.map(|(key, &mut (ref value, _, _))| {
            LruCache::<Key, Value>::update_key(self.list, key);
            (key, value)
        })
    }
}

/// An iterator over an `LruCache`'s entries that does not modify the timestamp.
pub struct PeekIter<'a, Key: 'a, Value: 'a> {
    map_iter: btree_map::Iter<'a, Key, (Value, Instant, usize)>,
}

impl<'a, Key, Value> Iterator for PeekIter<'a, Key, Value>
where
    Key: Ord + Clone,
{
    type Item = (&'a Key, &'a Value);

    fn next(&mut self) -> Option<(&'a Key, &'a Value)> {
        let now = Instant::now();
        let not_expired = self.map_iter.find(|&(_, &(_, instant, _))| instant > now);
        not_expired.map(|(key, &(ref value, _, _))| (key, value))
    }
}

/// Implementation of [LRU cache](index.html#least-recently-used-lru-cache).
#[derive(Debug)]
pub struct LruCache<Key, Value> {
    // Store the value itself, the expires date and a memory size of the value.
    // @todo make this a proper struct instead of an anonymous tuple.
    map: BTreeMap<Key, (Value, Instant, usize)>,
    list: VecDeque<Key>,
    // Maximum memory constraint.
    max_memory_size: usize,
    // Current memory usage, initialized with 0. Increased whenever an item is
    // inserted into the cache. Decreases when an item is removed or expires.
    current_memory_size: usize,
}

impl<Key, Value> LruCache<Key, Value>
where
    Key: Ord + Clone,
{
    /// Constructor for a mmemory constrained cache.
    pub fn with_memory_size(memory_size: usize) -> LruCache<Key, Value> {
        LruCache {
            map: BTreeMap::new(),
            list: VecDeque::new(),
            max_memory_size: memory_size,
            current_memory_size: 0,
        }
    }

    /// Inserts a key-value pair into the cache.
    ///
    /// If the key already existed in the cache, the existing value is returned and overwritten in
    /// the cache.  Otherwise, the key-value pair is inserted and `None` is returned.
    pub fn insert(
        &mut self,
        key: Key,
        value: Value,
        memory_size: usize,
        expires: Instant,
    ) -> Option<Value> {
        self.remove_expired();
        let old_value = self.remove(&key);

        if memory_size <= self.max_memory_size {
            // Remove old cache entries until we have room to insert the new item.
            while self.max_memory_size < self.current_memory_size + memory_size {
                let remove_key = self
                    .list
                    .pop_front()
                    .expect("Queue is empty but current memory size > 0");
                let (_, _, removed_size) = self
                    .map
                    .remove(&remove_key)
                    .expect("Shrinking cache failed");
                self.current_memory_size -= removed_size;
            }
            self.list.push_back(key.clone());

            self.current_memory_size += memory_size;
            let _ = self.map.insert(key, (value, expires, memory_size));
        }
        old_value
    }

    /// Removes a key-value pair from the cache.
    pub fn remove<Q: ?Sized>(&mut self, key: &Q) -> Option<Value>
    where
        Key: Borrow<Q>,
        Q: Ord,
    {
        self.map.remove(key).map(|(value, _, memory_size)| {
            let _ = self
                .list
                .iter()
                .position(|l| l.borrow() == key)
                .map(|p| self.list.remove(p));
            self.current_memory_size -= memory_size;
            value
        })
    }

    /// Clears the `LruCache`, removing all values.
    pub fn clear(&mut self) {
        self.map.clear();
        self.list.clear();
        self.current_memory_size = 0;
    }

    /// Retrieves a reference to the value stored under `key`, or `None` if the key doesn't exist.
    /// Also removes expired elements and updates the time.
    pub fn get<Q: ?Sized>(&mut self, key: &Q) -> Option<&Value>
    where
        Key: Borrow<Q>,
        Q: Ord,
    {
        self.remove_expired();

        let list = &mut self.list;
        self.map.get_mut(key).map(|result| {
            Self::update_key(list, key);
            &result.0
        })
    }

    /// Returns a reference to the value with the given `key`, if present and not expired, without
    /// updating the timestamp.
    pub fn peek<Q: ?Sized>(&self, key: &Q) -> Option<&Value>
    where
        Key: Borrow<Q>,
        Q: Ord,
    {
        self.map
            .get(key)
            .into_iter()
            .find(|&(_, t, _)| *t >= Instant::now())
            .map(|&(ref value, _, _)| value)
    }

    /// Returns whether `key` exists in the cache or not.
    pub fn contains_key<Q: ?Sized>(&self, key: &Q) -> bool
    where
        Key: Borrow<Q>,
        Q: Ord,
    {
        self.peek(key).is_some()
    }

    /// Returns the size of the cache, i.e. the number of cached non-expired key-value pairs.
    pub fn len(&self) -> usize {
        self.map
            .iter()
            .filter(|&(_, (_, t, _))| *t >= Instant::now())
            .count()
    }

    /// Returns `true` if there are no non-expired entries in the cache.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns an iterator over all entries that updates the timestamps as values are
    /// traversed. Also removes expired elements before creating the iterator.
    pub fn iter(&mut self) -> Iter<Key, Value> {
        self.remove_expired();

        Iter {
            map_iter_mut: self.map.iter_mut(),
            list: &mut self.list,
        }
    }

    /// Returns an iterator over all entries that does not modify the timestamps.
    pub fn peek_iter(&self) -> PeekIter<Key, Value> {
        PeekIter {
            map_iter: self.map.iter(),
        }
    }

    // Move `key` in the ordered list to the last
    fn update_key<Q: ?Sized>(list: &mut VecDeque<Key>, key: &Q)
    where
        Key: Borrow<Q>,
        Q: Ord,
    {
        if let Some(pos) = list.iter().position(|k| k.borrow() == key) {
            let _ = list.remove(pos).map(|it| list.push_back(it));
        }
    }

    fn remove_expired(&mut self) {
        // Because of the borrow checker we need to clone the keys to be removed
        // while accessing the map. Any better ideas how to simplify this?
        let remove_entries = self
            .map
            .iter()
            .filter(|(_, (_, t, _))| *t < Instant::now())
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in remove_entries {
            let _ = self.remove(&key);
        }
    }
}

impl<Key, Value> Clone for LruCache<Key, Value>
where
    Key: Clone,
    Value: Clone,
{
    fn clone(&self) -> LruCache<Key, Value> {
        LruCache {
            map: self.map.clone(),
            list: self.list.clone(),
            max_memory_size: self.max_memory_size,
            current_memory_size: self.current_memory_size,
        }
    }
}

#[cfg(test)]
mod test {
    use super::rand;
    use std::time::{Duration, Instant};

    #[cfg(feature = "fake_clock")]
    fn sleep(time: u64) {
        use fake_clock::FakeClock;
        FakeClock::advance_time(time);
    }

    #[cfg(not(feature = "fake_clock"))]
    fn sleep(time: u64) {
        use std::thread;
        thread::sleep(Duration::from_millis(time));
    }

    fn generate_random_vec<T>(len: usize) -> Vec<T>
    where
        T: rand::Rand,
    {
        let mut vec = Vec::<T>::with_capacity(len);
        for _ in 0..len {
            vec.push(rand::random());
        }
        vec
    }

    #[test]
    fn memory_size() {
        let size = 10usize;
        let mut lru_cache = super::LruCache::<usize, usize>::with_memory_size(size);

        for i in 0..10 {
            assert_eq!(lru_cache.len(), i);
            let _ = lru_cache.insert(i, i, 1, Instant::now() + Duration::from_secs(1000));
            assert_eq!(lru_cache.len(), i + 1);
        }

        for i in 10..1000 {
            let _ = lru_cache.insert(i, i, 1, Instant::now() + Duration::from_secs(1000));
            assert_eq!(lru_cache.len(), size);
        }

        for _ in (0..1000).rev() {
            assert!(lru_cache.contains_key(&(1000 - 1)));
            assert!(lru_cache.get(&(1000 - 1)).is_some());
            assert_eq!(*lru_cache.get(&(1000 - 1)).unwrap(), 1000 - 1);
        }
    }

    #[test]
    fn expiration_time() {
        let time_to_live = Duration::from_millis(100);
        let mut lru_cache = super::LruCache::<usize, usize>::with_memory_size(100usize);

        for i in 0..10 {
            assert_eq!(lru_cache.len(), i);
            let _ = lru_cache.insert(i, i, 1, Instant::now() + time_to_live);
            assert_eq!(lru_cache.len(), i + 1);
        }

        sleep(101);
        let _ = lru_cache.insert(11, 11, 1, Instant::now() + time_to_live);

        assert_eq!(lru_cache.len(), 1);

        for i in 0..10 {
            println!("{:#?}", lru_cache);
            assert!(!lru_cache.is_empty());
            assert_eq!(lru_cache.len(), i + 1);
            let _ = lru_cache.insert(i, i, 1, Instant::now() + time_to_live);
            assert_eq!(lru_cache.len(), i + 2);
        }

        sleep(101);
        assert_eq!(0, lru_cache.len());
        assert!(lru_cache.is_empty());
    }

    /*#[test]
    fn time_only_check() {
        let time_to_live = Duration::from_millis(50);
        let mut lru_cache = super::LruCache::<usize, usize>::with_expiry_duration(time_to_live);
    
        assert_eq!(lru_cache.len(), 0);
        let _ = lru_cache.insert(0, 0);
        assert_eq!(lru_cache.len(), 1);
    
        sleep(101);
    
        assert!(!lru_cache.contains_key(&0));
        assert_eq!(lru_cache.len(), 0);
    }
    
    #[test]
    fn time_and_size() {
        let size = 10usize;
        let time_to_live = Duration::from_millis(100);
        let mut lru_cache =
            super::LruCache::<usize, usize>::with_expiry_duration_and_capacity(time_to_live, size);
    
        for i in 0..1000 {
            if i < size {
                assert_eq!(lru_cache.len(), i);
            }
    
            let _ = lru_cache.insert(i, i);
    
            if i < size {
                assert_eq!(lru_cache.len(), i + 1);
            } else {
                assert_eq!(lru_cache.len(), size);
            }
        }
    
        sleep(101);
        let _ = lru_cache.insert(1, 1);
    
        assert_eq!(lru_cache.len(), 1);
    }
    
    #[derive(PartialEq, PartialOrd, Ord, Clone, Eq)]
    struct Temp {
        id: Vec<u8>,
    }
    
    #[test]
    fn time_size_struct_value() {
        let size = 100usize;
        let time_to_live = Duration::from_millis(100);
    
        let mut lru_cache =
            super::LruCache::<Temp, usize>::with_expiry_duration_and_capacity(time_to_live, size);
    
        for i in 0..1000 {
            if i < size {
                assert_eq!(lru_cache.len(), i);
            }
    
            let _ = lru_cache.insert(
                Temp {
                    id: generate_random_vec::<u8>(64),
                },
                i,
            );
    
            if i < size {
                assert_eq!(lru_cache.len(), i + 1);
            } else {
                assert_eq!(lru_cache.len(), size);
            }
        }
    
        sleep(101);
        let _ = lru_cache.insert(
            Temp {
                id: generate_random_vec::<u8>(64),
            },
            1,
        );
    
        assert_eq!(lru_cache.len(), 1);
    }
    
    #[test]
    fn iter() {
        let mut lru_cache = super::LruCache::<usize, usize>::with_capacity(3);
    
        let _ = lru_cache.insert(0, 0);
        sleep(1);
        let _ = lru_cache.insert(1, 1);
        sleep(1);
        let _ = lru_cache.insert(2, 2);
        sleep(1);
    
        assert_eq!(
            vec![(&0, &0), (&1, &1), (&2, &2)],
            lru_cache.iter().collect::<Vec<_>>()
        );
    
        let initial_instant0 = lru_cache.map[&0].1;
        let initial_instant2 = lru_cache.map[&2].1;
        sleep(1);
    
        // only the first two entries should have their timestamp updated (and position in list)
        let _ = lru_cache.iter().take(2).all(|_| true);
    
        assert_ne!(lru_cache.map[&0].1, initial_instant0);
        assert_eq!(lru_cache.map[&2].1, initial_instant2);
    
        assert_eq!(*lru_cache.list.front().unwrap(), 2);
        assert_eq!(*lru_cache.list.back().unwrap(), 1);
    }
    
    #[test]
    fn peek_iter() {
        let time_to_live = Duration::from_millis(500);
        let mut lru_cache = super::LruCache::<usize, usize>::with_expiry_duration(time_to_live);
    
        let _ = lru_cache.insert(0, 0);
        let _ = lru_cache.insert(2, 2);
        let _ = lru_cache.insert(3, 3);
    
        sleep(300);
        assert_eq!(
            vec![(&0, &0), (&2, &2), (&3, &3)],
            lru_cache.peek_iter().collect::<Vec<_>>()
        );
        assert_eq!(Some(&2), lru_cache.get(&2));
        let _ = lru_cache.insert(1, 1);
        let _ = lru_cache.insert(4, 4);
    
        sleep(300);
        assert_eq!(
            vec![(&1, &1), (&2, &2), (&4, &4)],
            lru_cache.peek_iter().collect::<Vec<_>>()
        );
    
        sleep(300);
        assert!(lru_cache.is_empty());
    }
    
    #[test]
    fn update_time_check() {
        let time_to_live = Duration::from_millis(500);
        let mut lru_cache = super::LruCache::<usize, usize>::with_expiry_duration(time_to_live);
    
        assert_eq!(lru_cache.len(), 0);
        let _ = lru_cache.insert(0, 0);
        assert_eq!(lru_cache.len(), 1);
    
        sleep(300);
        assert_eq!(Some(&0), lru_cache.get(&0));
        sleep(300);
        assert_eq!(Some(&0), lru_cache.peek(&0));
        sleep(300);
        assert_eq!(None, lru_cache.peek(&0));
    }
    
    #[test]
    fn deref_coercions() {
        let mut lru_cache = super::LruCache::<String, usize>::with_capacity(1);
        let _ = lru_cache.insert("foo".to_string(), 0);
        assert_eq!(true, lru_cache.contains_key("foo"));
        assert_eq!(Some(&0), lru_cache.get("foo"));
        assert_eq!(Some(&mut 0), lru_cache.get_mut("foo"));
        assert_eq!(Some(&0), lru_cache.peek("foo"));
        assert_eq!(Some(0), lru_cache.remove("foo"));
    }*/
}
