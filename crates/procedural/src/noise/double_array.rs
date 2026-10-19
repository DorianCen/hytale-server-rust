// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/DoubleArray.java`
//! Java `DoubleArray.Double2` / `DoubleArray.Double3` value holders.

/// Java `DoubleArray.Double2`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Double2 {
    pub x: f64,
    pub y: f64,
}

/// Java `DoubleArray.Double3`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Double3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
