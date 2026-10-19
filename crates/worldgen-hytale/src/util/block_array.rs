// Ported from: `.hytale-source/.../worldgen/util/BlockArray.java`
//! Java `com.hypixel.hytale.server.worldgen.util.BlockArray` — a read-only view
//! over a set of block ids.

/// Java `BlockArray`.
pub trait BlockArray {
    /// Java `int[] getBlocks()`.
    fn get_blocks(&self) -> &[i32];

    /// Java `int size()`.
    fn size(&self) -> usize;

    /// Java `boolean contains(int block)`.
    fn contains(&self, block: i32) -> bool;
}

impl<T: BlockArray + ?Sized> BlockArray for &T {
    fn get_blocks(&self) -> &[i32] {
        (**self).get_blocks()
    }

    fn size(&self) -> usize {
        (**self).size()
    }

    fn contains(&self, block: i32) -> bool {
        (**self).contains(block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestArray {
        blocks: Vec<i32>,
    }

    impl BlockArray for TestArray {
        fn get_blocks(&self) -> &[i32] {
            &self.blocks
        }

        fn size(&self) -> usize {
            self.blocks.len()
        }

        fn contains(&self, block: i32) -> bool {
            self.blocks.contains(&block)
        }
    }

    #[test]
    fn block_array_exposes_blocks_size_and_membership() {
        let array = TestArray {
            blocks: vec![3, 5, 7],
        };
        assert_eq!(array.get_blocks(), &[3, 5, 7]);
        assert_eq!(array.size(), 3);
        assert!(array.contains(5));
        assert!(!array.contains(4));
    }

    #[test]
    fn blanket_impl_for_references_behaves_the_same() {
        let array = TestArray { blocks: vec![11] };
        let reference: &TestArray = &array;
        assert_eq!(reference.size(), 1);
        assert!(reference.contains(11));
        assert_eq!(reference.get_blocks(), &[11]);
    }
}
