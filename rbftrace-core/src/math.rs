
use std::{cmp::Ordering, iter::Cloned};

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum ClosedInterval<T> {
    Empty,
    NotEmpty(T, T),
    NotAnInterval
}

pub fn partial_max<T:PartialOrd>(a:T, b: T) -> Option<T> {
    a.partial_cmp(&b)
     .and_then(|cmp| match cmp {
        Ordering::Greater => Some(a),
        _ => Some(b)
    })
}

pub fn partial_min<T:PartialOrd>(a:T, b: T) -> Option<T> {
    a.partial_cmp(&b)
     .and_then(|cmp| match cmp {
        Ordering::Less => Some(a),
        _ => Some(b)
    })
}

impl <T> ClosedInterval<T> 
where T: PartialEq + PartialOrd + Clone + Copy
{
    pub fn closed(lower: T, upper: T) -> Self {
        ClosedInterval::NotEmpty(lower, upper)
    }

    pub fn empty() -> Self {
        ClosedInterval::Empty
    }

    pub fn is_empty(&self) -> bool {
        match self {
            ClosedInterval::Empty => true,
            _ => false
        }
    }
    
    pub fn is_interval(&self) -> bool {
        match self {
            ClosedInterval::NotAnInterval => false,
            _ => true 
        }
    }

    pub fn get_upper(&self) -> Option<T> {
        match self {
            ClosedInterval::NotEmpty(_, b) => Some(*b),
            _ => None
        }
    }
    
    pub fn get_lower(&self) -> Option<T> {
        match self {
            ClosedInterval::NotEmpty(a, _) => Some(*a),
            _ => None
        }
    }

    pub fn overlaps_with(&self, other: &Self) -> bool {
        let intersection = self.intersection(other);

        intersection.is_interval() && !intersection.is_empty()
    }

    pub fn intersection(&self, other: &Self) -> Self {
        if !(self.is_interval() || other.is_interval()) {
            return Self::NotAnInterval;
        }

        let lower = self.get_lower()
                                 .zip(other.get_lower())
                                 .map(|(a,b)| partial_max(a, b).unwrap());

        let upper = self.get_upper()
                             .zip(other.get_upper())
                             .map(|(a,b)| partial_min(a, b).unwrap());
        
        lower.zip(upper)
             .map_or(Self::Empty, |(l, u)| {
                 if l <= u {
                     Self::closed(l, u)
                 } else {
                    Self::Empty
                 }
             })
    }

    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() {
            return *other;
        }

        if other.is_empty() {
            return *self
        }
 
        if !self.is_interval() || !other.is_interval() || !self.overlaps_with(other){
            return Self::NotAnInterval;
        }

        let lower = self.get_lower()
                        .zip(other.get_lower())
                        .map(|(a,b)| partial_min(a, b).unwrap())
                        .unwrap();

        let upper = self.get_upper()
                        .zip(other.get_upper())
                        .map(|(a,b)| partial_max(a, b).unwrap())
                        .unwrap();
        
        Self::NotEmpty(lower, upper)
    }
}

#[cfg(test)]
mod tests {
    use crate::math::ClosedInterval;

    #[test]
    fn test_interval(){
        assert!(ClosedInterval::<i32>::empty().is_empty());
        assert!(ClosedInterval::<i32>::empty().is_interval());

        let a = ClosedInterval::closed(1, 1);
        let b = ClosedInterval::closed(2, 4);
        let c = ClosedInterval::closed(3, 4);
        let d = ClosedInterval::closed(2, 3);

        assert_eq!(a.intersection(&a), a);
        assert_eq!(b.intersection(&b), b);
        assert_eq!(c.intersection(&c), c);
        assert_eq!(d.intersection(&d), d);

        assert_eq!(b.intersection(&c), c);
        assert_eq!(c.intersection(&d), ClosedInterval::closed(3, 3));
        assert!(a.intersection(&b).is_empty());

        assert!(a.union(&a) == a);
        assert!(a.union(&ClosedInterval::empty()) == a);
        assert!(ClosedInterval::<i32>::empty().union(&ClosedInterval::empty()).is_empty());
        assert!(ClosedInterval::empty().union(&a) == a);
        assert!(!a.union(&b).is_interval());
        assert_eq!(c.union(&d), b);
    }
}