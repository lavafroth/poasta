extern crate num;
use num::{FromPrimitive, PrimInt};

use std::cmp;
use std::fmt::Debug;
use std::ops::{Index, IndexMut};

pub mod aligner;
pub mod compute;

pub type DiagIx = i64;

pub trait OffsetPrimitive: FromPrimitive + PrimInt
    + TryInto<usize> + Default + Copy + Debug { }

impl OffsetPrimitive for u8 { }
impl OffsetPrimitive for u16 { }
impl OffsetPrimitive for u32 { }
impl OffsetPrimitive for u64 { }

pub type FRPoint<Offset> = (DiagIx, Offset);
pub type FRPointContainer<Offset> = Vec<Option<Offset>>;

trait WFPoint<Offset: OffsetPrimitive>: Debug {
    fn rank(&self) -> usize;
    fn offset(&self) -> Offset;
    fn diag(&self) -> DiagIx;
}

impl<Offset: OffsetPrimitive> WFPoint<Offset> for FRPoint<Offset> {
    fn rank(&self) -> usize {
        match self.1.try_into() {
            Ok(ref v) => v - self.0 as usize,
            Err(_) => panic!("Could not convert offset to usize!")
        }
    }

    fn offset(&self) -> Offset {
        self.1
    }

    fn diag(&self) -> DiagIx {
        self.0
    }
}

#[derive(Debug, Clone)]
struct Wavefront<Offset: OffsetPrimitive> {
    pub k_lo: DiagIx,
    pub k_hi: DiagIx,

    furthest_points: FRPointContainer<Offset>,
}

impl<Offset: OffsetPrimitive> Wavefront<Offset> {
    pub fn new() -> Self {
        Wavefront::default()
    }

    pub fn new_with_fr_points(k_lo: DiagIx, k_hi: DiagIx,
                              fr_points: FRPointContainer<Offset>) -> Self {
        // Check which diagonal has first/last furthest point, and use that as k_lo and k_hi
        // respectively
        let mut actual_k_lo = None;
        let mut actual_k_hi = None;

        for (k, offset) in (k_lo..=k_hi).zip(&fr_points) {
            if offset.is_some() {
                if actual_k_lo.is_none() {
                    actual_k_lo = Some(k);
                }

                actual_k_hi = Some(k);
            }
        }

        if let (Some(new_k_lo), Some(new_k_hi)) = (actual_k_lo, actual_k_hi) {
            let start = (new_k_lo - k_lo) as usize;
            let end = (new_k_hi - k_lo) as usize;

            Self {
                k_lo: new_k_lo,
                k_hi: new_k_hi,
                furthest_points: fr_points[start..=end].into()
            }
        } else {
            Self::default()
        }
    }

    fn diag_to_ix(&self, diag: DiagIx) -> Option<usize> {
        if diag >= self.k_lo && diag <= self.k_hi {
            Some((diag - self.k_lo) as usize)
        } else {
            None
        }
    }

    pub fn update_fr_point(&mut self, point: FRPoint<Offset>) {
        let ix = self.diag_to_ix(point.diag()).unwrap();

        match self.furthest_points[ix] {
            Some(ref mut v) => *v = cmp::max(*v, point.offset()),
            None => self.furthest_points[ix as usize] = Some(point.offset())
        };
    }

    pub fn get_fr_point(&self, k: DiagIx) -> Option<FRPoint<Offset>> {
        let ix = self.diag_to_ix(k)?;

        return if let Some(offset) = &self.furthest_points[ix] {
            Some((k, offset.clone()))
        } else {
            None
        }
    }

    pub fn iter(&self) -> impl Iterator<Item=FRPoint<Offset>> + '_ {
        self.furthest_points.iter()
            .zip(self.k_lo..=self.k_hi)
            .filter_map(|v| match v {
                (Some(offset), diag) => Some((diag, offset.clone())),
                _ => None
            })
    }
}

impl<Offset: OffsetPrimitive> Default for Wavefront<Offset> {
    fn default() -> Self {
        Wavefront {
            k_lo: 0,
            k_hi: 0,
            furthest_points: vec![None],
        }
    }
}

impl<Offset: OffsetPrimitive> Index<DiagIx> for Wavefront<Offset> {
    type Output = Option<Offset>;

    fn index(&self, index: DiagIx) -> &Self::Output {
        assert!(index >= self.k_lo && index <= self.k_hi);

        &self.furthest_points[(index - self.k_lo) as usize]
    }
}

impl<Offset: OffsetPrimitive> IndexMut<DiagIx> for Wavefront<Offset> {
    fn index_mut(&mut self, index: DiagIx) -> &mut Self::Output {
        assert!(index >= self.k_lo && index <= self.k_hi);

        &mut self.furthest_points[(index - self.k_lo) as usize]
    }
}



