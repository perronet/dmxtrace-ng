use linked_list::*;
use crate::time::*;

use super::Point;

#[derive(Debug, Clone)]
pub struct SparseMap {
    pub buckets : Vec<LinkedList<Point>>,
    pub capacity : usize,
    pub bucket_size: u64,
    pub count : u64,
}

impl SparseMap {
    pub fn add(&mut self, p : Point) {
        self.update_map(p, true);
    }

    pub fn insert(&mut self, p : Point) {
        self.update_map(p, false);
    }

    fn update_map(&mut self, p : Point, keep_monotonicity: bool) {
        while p.delta >= self.bucket_size * (self.capacity as u64) {
            self.double_buckets();
        }
        
        let bi = self.bucket_index_of(p.delta);
        let mut b = &mut (self.buckets[bi]);
        let mut c = b.cursor();

        let mut found = false;
        c.prev(); // Go to end
        let mut cost;
        loop {
            cost = c.peek_prev();
            match cost {
                None => break, //no cost found
                Some(point) => 
                {
                    if point.delta == p.delta {  // same cost found
                        point.cost = p.cost;
                        found = true;
                        c.prev(); // go back by one so that next() is the new point
                        break; 
                    }
                    else if point.delta < p.delta { // position found
                        c.insert(p);
                        self.count += 1;
                        found = true;
                        break;
                    }
                }
            };
            c.prev();
        }

        if !found { // either list is empty or p should be in first position
            c.reset();
            c.insert(p);
            self.count += 1;
        }

        // Ensuring monotonicity: removing all non increasing elements in the same bucket
        // Monotonicity can be broken only by a continuous sequence of elements starting from the newly
        // inserted element. So, we start by checking the current bucket. 
        // If the new element remains as last in the current bucket, we move to the following ones.
        if keep_monotonicity {
            match c.peek_next() {
                None => { panic! (); },
                Some (el) => { assert! (el.delta == p.delta && el.cost == p.cost); }
            }
            c.next(); // cursor is not at the following element
            let mut sequence_interrupted = false; 
            let mut cbi = bi; 
            loop {
                loop {
                    match c.peek_next() {
                        None => { break; },
                        Some (el) => { 
                            if el.cost <= p.cost { 
                                c.remove(); 
                            } 
                            else {
                                sequence_interrupted = true;
                                break;
                            }
                        }
                    }
                }
    
                // Move to the next bucket if the non increasing sequence didn't end
                if sequence_interrupted { break; }
                cbi += 1;
                if cbi >= self.capacity { break; }
                b = &mut (self.buckets[cbi]);
                c = b.cursor();
            }
        }
    }

    pub fn get(&self, delta: Duration) -> Cost {
        let max = self.bucket_size * (self.capacity as u64);
        if max <= delta { return 0; }

        let mut bi = self.bucket_index_of(delta); // start with biggest bucket index that could contain the cost
        loop {
            let b = &self.buckets[bi];
            for el in b.iter().rev() {
                if el.delta <= delta { return el.cost; } // found
            }

            if bi == 0 { return 0; } // not found
            bi -= 1;
        }
    }

    pub fn bucket_index_of(&self, delta : Duration) -> usize { 
        return (delta / self.bucket_size) as usize;
    }

    // Used when a new element cannot fit
    fn double_buckets(&mut self) {
        self.bucket_size *=2;
        for i in (0..self.capacity/2).step_by(1) {
            let mut l = LinkedList::new();
            l.append(&mut self.buckets[i*2]);
            if i*2 < self.capacity { l.append(&mut self.buckets[i*2+1]); }
            
            self.buckets[i].append(&mut l);
        }
    }

    pub fn new(capacity: usize) -> Self {
        let mut map = SparseMap {
            capacity : capacity,
            buckets : Vec::<LinkedList<Point>>::with_capacity(capacity),
            bucket_size : 1,
            count : 0,
        };

        for _ in 0..capacity {
            map.buckets.push(<LinkedList<Point>>::new());
        }

        map
    }
}

impl<'a> IntoIterator for &'a SparseMap {
    type Item = Point;
    type IntoIter = SparseMapIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        SparseMapIterator {
            map: self,
            bucket_idx: 0,
            list_iter: None,
        }
    }
}

pub struct SparseMapIterator<'a> {
    map: &'a SparseMap,
    bucket_idx: usize,
    list_iter: Option<linked_list::Iter<'a, Point>>,
}

impl<'a> Iterator for SparseMapIterator<'a> {
    type Item = Point;

    fn next(&mut self) -> Option<Point> {
        let mut try_next_bucket = false;

        while self.bucket_idx < self.map.capacity {
            let curr_list = &self.map.buckets[self.bucket_idx];

            if self.list_iter.is_none() || try_next_bucket {
                self.list_iter = Some(curr_list.iter());
            }

            if let Some(point) = self.list_iter.as_mut().unwrap().next() {
                return Some(*point);
            } else {
                // Try in the next bucket
                self.bucket_idx += 1;
                try_next_bucket = true;
            }
        }

        None
    }
}

impl PartialEq for SparseMap {
    fn eq(&self, other: &Self) -> bool {
        if self.count != other.count {
            return false
        }
        for (p1, p2) in self.into_iter().zip(other.into_iter()) {
            if p1 != p2 {
                return false
            }
        }

        return true
    }
}

impl Eq for SparseMap {}