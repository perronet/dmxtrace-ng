use std::cmp::Ordering;

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Interval<T> {
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

impl <T> Interval<T> 
where T: PartialEq + PartialOrd + Clone + Copy
{
    pub fn closed(lower: T, upper: T) -> Self {
        Interval::NotEmpty(lower, upper)
    }

    pub fn empty() -> Self {
        Interval::Empty
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Interval::Empty => true,
            _ => false
        }
    }
    
    pub fn is_interval(&self) -> bool {
        match self {
            Interval::NotAnInterval => false,
            _ => true 
        }
    }

    pub fn get_upper(&self) -> Option<T> {
        match self {
            Interval::NotEmpty(_, b) => Some(*b),
            _ => None
        }
    }
    
    pub fn get_lower(&self) -> Option<T> {
        match self {
            Interval::NotEmpty(a, _) => Some(*a),
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
    use crate::math::Interval;

    #[test]
    fn test_interval(){
        assert!(Interval::<i32>::empty().is_empty());
        assert!(Interval::<i32>::empty().is_interval());

        let a = Interval::closed(1, 1);
        let b = Interval::closed(2, 4);
        let c = Interval::closed(3, 4);
        let d = Interval::closed(2, 3);

        assert_eq!(a.intersection(&a), a);
        assert_eq!(b.intersection(&b), b);
        assert_eq!(c.intersection(&c), c);
        assert_eq!(d.intersection(&d), d);

        assert_eq!(b.intersection(&c), c);
        assert_eq!(c.intersection(&d), Interval::closed(3, 3));
        assert!(a.intersection(&b).is_empty());

        assert!(a.union(&a) == a);
        assert!(a.union(&Interval::empty()) == a);
        assert!(Interval::<i32>::empty().union(&Interval::empty()).is_empty());
        assert!(Interval::empty().union(&a) == a);
        assert!(!a.union(&b).is_interval());
        assert_eq!(c.union(&d), b);
    }
}