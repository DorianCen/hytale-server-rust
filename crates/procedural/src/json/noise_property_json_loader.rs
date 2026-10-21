// Ported from: `.hytale-source/.../procedurallib/json/NoisePropertyJsonLoader.java`
//! Java `NoisePropertyJsonLoader` — the composition loader driving the
//! `NoiseProperty` DSL. The trailing "generic" keywords (`Formula`, `Curve`,
//! `Scale`, `Normalize`, `Offset`, `Rotate` via `Pitch`/`Yaw`, `Gradient`)
//! are applied in the exact Java order, with the same
//! `type != X && has(...)` filter quirks.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::blend_noise_property_json_loader::BlendNoisePropertyJsonLoader;
use crate::json::coordinate_randomizer_json_loader::CoordinateRandomizerJsonLoader;
use crate::json::coordinate_rotator_json_loader::CoordinateRotatorJsonLoader;
use crate::json::curve_noise_property_json_loader::CurveNoisePropertyJsonLoader;
use crate::json::double_range_json_loader::DoubleRangeJsonLoader;
use crate::json::gradient_noise_property_json_loader::GradientNoisePropertyJsonLoader;
use crate::json::noise_function_json_loader::NoiseFunctionJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::property::fractal_noise_property::{FractalMode, FractalNoiseProperty};
use crate::noise::property::noise_formula_property::{NoiseFormula, NoiseFormulaProperty};
use crate::noise::property::noise_property_type::NoisePropertyType;
use crate::noise::property::single_noise_property::SingleNoiseProperty;
use crate::noise::property::NoiseProperty;
use crate::noise::property::{
    DistortedNoiseProperty, InvertNoiseProperty, MaxNoiseProperty, MinNoiseProperty,
    MultiplyNoiseProperty, NormalizeNoiseProperty, OffsetNoiseProperty, RotateNoiseProperty,
    ScaleNoiseProperty, SumNoiseProperty, SumNoisePropertyEntry,
};

/// Java `NoisePropertyJsonLoader`.
pub struct NoisePropertyJsonLoader {
    /// Java `extends JsonLoader<K, NoiseProperty>`.
    pub base: JsonLoaderBase,
}

/// Java `NoisePropertyJsonLoader.Constants` defaults.
const DEFAULT_FRACTAL_MODE: FractalMode = FractalMode::Fbm;
const DEFAULT_LACUNARITY: f64 = 2.0;
const DEFAULT_PERSISTENCE: f64 = 0.5;

impl NoisePropertyJsonLoader {
    /// Java `NoisePropertyJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".NoiseProperty"), data_folder, json),
        }
    }

    /// Java `NoiseProperty load()`.
    pub fn load(&self) -> Rc<dyn NoiseProperty> {
        const KEY_TYPE: &str = "Type";
        const KEY_NOISE: &str = "Noise";
        const KEY_SUM_FACTORS: &str = "Factors";
        const KEY_SCALE: &str = "Scale";
        const KEY_FORMULA: &str = "Formula";
        const KEY_DISTORTED_RANDOMIZER: &str = "Randomizer";
        const KEY_NORMALIZE_RANGE: &str = "Range";
        const KEY_OFFSET: &str = "Offset";
        const KEY_OFFSET_X: &str = "OffsetX";
        const KEY_OFFSET_Y: &str = "OffsetY";
        const KEY_OFFSET_Z: &str = "OffsetZ";
        const KEY_ROTATE: &str = "Rotate";
        const KEY_CURVE: &str = "Curve";
        const KEY_NORMALIZE: &str = "Normalize";
        const KEY_GRADIENT: &str = "Gradient";
        const KEY_PITCH: &str = "Pitch";
        const KEY_YAW: &str = "Yaw";
        const KEY_SEED: &str = "Seed";
        const KEY_OCTAVES: &str = "Octaves";
        const KEY_FRACTAL_MODE: &str = "FractalMode";
        const KEY_LACUNARITY: &str = "Lacunarity";
        const KEY_PERSISTENCE: &str = "Persistence";

        let mut noise_property: Rc<dyn NoiseProperty>;
        let mut property_type: Option<NoisePropertyType> = None;

        if self.base.has(KEY_TYPE) {
            let type_name = JsonLoaderBase::get_as_string(&self.base.get(KEY_TYPE).unwrap());
            let parsed = Self::noise_property_type_value_of(&type_name);
            property_type = Some(parsed);
            noise_property = match parsed {
                NoisePropertyType::Max => {
                    if !self.base.has(KEY_NOISE) {
                        panic!("Could not find noise map data. Keyword: {KEY_NOISE}");
                    }
                    Rc::new(MaxNoiseProperty::new(
                        self.load_noise_properties(&self.base.get(KEY_NOISE).unwrap()),
                    ))
                }
                NoisePropertyType::Min => {
                    if !self.base.has(KEY_NOISE) {
                        panic!("Could not find noise map data. Keyword: {KEY_NOISE}");
                    }
                    Rc::new(MinNoiseProperty::new(
                        self.load_noise_properties(&self.base.get(KEY_NOISE).unwrap()),
                    ))
                }
                NoisePropertyType::Sum => {
                    if !self.base.has(KEY_NOISE) {
                        panic!("Could not find noise map data. Keyword: {KEY_NOISE}");
                    }
                    if !self.base.has(KEY_SUM_FACTORS) {
                        panic!("Could not find factors for sum composed noise map. Keyword: {KEY_SUM_FACTORS}");
                    }
                    let noise_properties =
                        self.load_noise_properties(&self.base.get(KEY_NOISE).unwrap());
                    let factors = self
                        .load_double_array(self.base.get(KEY_SUM_FACTORS), noise_properties.len());
                    let entries: Vec<SumNoisePropertyEntry> = noise_properties
                        .into_iter()
                        .zip(factors)
                        .map(|(property, factor)| SumNoisePropertyEntry::new(property, factor))
                        .collect();
                    Rc::new(SumNoiseProperty::new(entries))
                }
                NoisePropertyType::Scale => {
                    if !self.base.has(KEY_NOISE) {
                        panic!("Could not find noise map data. Keyword: {KEY_NOISE}");
                    }
                    if !self.base.has(KEY_SCALE) {
                        panic!(
                            "Could not find scale data for scaled noise map. Keyword: {KEY_SCALE}"
                        );
                    }
                    let noise = NoisePropertyJsonLoader::new(
                        self.base.seed.clone(),
                        &self.base.data_folder,
                        self.base.get(KEY_NOISE),
                    )
                    .load();
                    let scale = JsonLoaderBase::get_as_double(&self.base.get(KEY_SCALE).unwrap());
                    // Java `new ScaleNoiseProperty(noise, scale)` = uniform.
                    Rc::new(ScaleNoiseProperty::uniform(noise, scale))
                }
                NoisePropertyType::Formula => {
                    if !self.base.has(KEY_NOISE) {
                        panic!("Could not find noise map data. Keyword: {KEY_NOISE}");
                    }
                    if !self.base.has(KEY_FORMULA) {
                        panic!("Could not find formula type for noise map. Keyword: {KEY_FORMULA}");
                    }
                    let noise = NoisePropertyJsonLoader::new(
                        self.base.seed.clone(),
                        &self.base.data_folder,
                        self.base.get(KEY_NOISE),
                    )
                    .load();
                    let formula = Self::noise_formula_value_of(&JsonLoaderBase::get_as_string(
                        &self.base.get(KEY_FORMULA).unwrap(),
                    ));
                    Rc::new(NoiseFormulaProperty::new(noise, Rc::new(formula)))
                }
                NoisePropertyType::Multiply => {
                    if !self.base.has(KEY_NOISE) {
                        panic!("Could not find noise map data. Keyword: {KEY_NOISE}");
                    }
                    Rc::new(MultiplyNoiseProperty::new(
                        self.load_noise_properties(&self.base.get(KEY_NOISE).unwrap()),
                    ))
                }
                NoisePropertyType::Distorted => {
                    if !self.base.has(KEY_NOISE) {
                        panic!("Could not find noise map data. Keyword: {KEY_NOISE}");
                    }
                    if !self.base.has(KEY_DISTORTED_RANDOMIZER) {
                        panic!("Could not find randomizer for distorted noise map. Keyword: {KEY_DISTORTED_RANDOMIZER}");
                    }
                    let noise = NoisePropertyJsonLoader::new(
                        self.base.seed.clone(),
                        &self.base.data_folder,
                        self.base.get(KEY_NOISE),
                    )
                    .load();
                    Rc::new(DistortedNoiseProperty::new(
                        noise,
                        CoordinateRandomizerJsonLoader::new(
                            self.base.seed.clone(),
                            &self.base.data_folder,
                            self.base.get(KEY_DISTORTED_RANDOMIZER),
                        )
                        .load(),
                    ))
                }
                NoisePropertyType::Normalize => {
                    if !self.base.has(KEY_NOISE) {
                        panic!("Could not find noise map data. Keyword: {KEY_NOISE}");
                    }
                    if !self.base.has(KEY_NORMALIZE_RANGE) {
                        panic!("Could not find range data for normalized noise map. Keyword: {KEY_NORMALIZE_RANGE}");
                    }
                    let noise = NoisePropertyJsonLoader::new(
                        self.base.seed.clone(),
                        &self.base.data_folder,
                        self.base.get(KEY_NOISE),
                    )
                    .load();
                    let range = DoubleRangeJsonLoader::new(
                        self.base.seed.clone(),
                        &self.base.data_folder,
                        self.base.get(KEY_NORMALIZE_RANGE),
                    )
                    .load();
                    use crate::noise::supplier::IDoubleRange;
                    Rc::new(NormalizeNoiseProperty::new(
                        noise,
                        range.get_value(0.0),
                        range.get_value(1.0) - range.get_value(0.0),
                    ))
                }
                NoisePropertyType::Invert => {
                    if !self.base.has(KEY_NOISE) {
                        panic!("Could not find noise map data. Keyword: {KEY_NOISE}");
                    }
                    let noise = NoisePropertyJsonLoader::new(
                        self.base.seed.clone(),
                        &self.base.data_folder,
                        self.base.get(KEY_NOISE),
                    )
                    .load();
                    Rc::new(InvertNoiseProperty::new(noise))
                }
                NoisePropertyType::Offset => {
                    if !self.base.has(KEY_NOISE) {
                        panic!("Could not find noise map data. Keyword: {KEY_NOISE}");
                    }
                    let noise = NoisePropertyJsonLoader::new(
                        self.base.seed.clone(),
                        &self.base.data_folder,
                        self.base.get(KEY_NOISE),
                    )
                    .load();
                    let offset = if self.base.has(KEY_OFFSET) {
                        JsonLoaderBase::get_as_double(&self.base.get(KEY_OFFSET).unwrap())
                    } else {
                        0.0
                    };
                    let offset_x = if self.base.has(KEY_OFFSET_X) {
                        JsonLoaderBase::get_as_double(&self.base.get(KEY_OFFSET_X).unwrap())
                    } else {
                        offset
                    };
                    let offset_y = if self.base.has(KEY_OFFSET_Y) {
                        JsonLoaderBase::get_as_double(&self.base.get(KEY_OFFSET_Y).unwrap())
                    } else {
                        offset
                    };
                    let offset_z = if self.base.has(KEY_OFFSET_Z) {
                        JsonLoaderBase::get_as_double(&self.base.get(KEY_OFFSET_Z).unwrap())
                    } else {
                        offset
                    };
                    Rc::new(OffsetNoiseProperty::new(
                        noise, offset_x, offset_y, offset_z,
                    ))
                }
                NoisePropertyType::Rotate => {
                    if !self.base.has(KEY_NOISE) {
                        panic!("Could not find noise map data. Keyword: {KEY_NOISE}");
                    }
                    if !self.base.has(KEY_ROTATE) {
                        panic!("Could not find noise map data. Keyword: {KEY_NOISE}");
                    }
                    let noise = NoisePropertyJsonLoader::new(
                        self.base.seed.clone(),
                        &self.base.data_folder,
                        self.base.get(KEY_NOISE),
                    )
                    .load();
                    let rotation = CoordinateRotatorJsonLoader::new(
                        self.base.seed.clone(),
                        &self.base.data_folder,
                        self.base.get(KEY_ROTATE),
                    )
                    .load();
                    Rc::new(RotateNoiseProperty::with_rotator(
                        noise,
                        rotation.into_matrix_rotator(),
                    ))
                }
                NoisePropertyType::Gradient => {
                    if !self.base.has(KEY_NOISE) {
                        panic!("Could not find noise map data. Keyword: {KEY_NOISE}");
                    }
                    let noise = NoisePropertyJsonLoader::new(
                        self.base.seed.clone(),
                        &self.base.data_folder,
                        self.base.get(KEY_NOISE),
                    )
                    .load();
                    Rc::new(
                        GradientNoisePropertyJsonLoader::new(
                            self.base.seed.clone(),
                            &self.base.data_folder,
                            self.base.json.clone(),
                            noise,
                        )
                        .load(),
                    ) as Rc<dyn NoiseProperty>
                }
                NoisePropertyType::Curve => Rc::new(
                    CurveNoisePropertyJsonLoader::new(
                        self.base.seed.clone(),
                        &self.base.data_folder,
                        self.base.json.clone(),
                        None,
                    )
                    .load(),
                ) as Rc<dyn NoiseProperty>,
                NoisePropertyType::Blend => Rc::new(
                    BlendNoisePropertyJsonLoader::new(
                        self.base.seed.clone(),
                        &self.base.data_folder,
                        self.base.json.clone(),
                    )
                    .load(),
                ) as Rc<dyn NoiseProperty>,
            };
        } else {
            let noise_function = NoiseFunctionJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.json.clone(),
            )
            .load();
            if self.base.has(KEY_OCTAVES) {
                let fractal_mode = if self.base.has(KEY_FRACTAL_MODE) {
                    Self::fractal_mode_value_of(&JsonLoaderBase::get_as_string(
                        &self.base.get(KEY_FRACTAL_MODE).unwrap(),
                    ))
                } else {
                    DEFAULT_FRACTAL_MODE
                };
                let octaves = JsonLoaderBase::get_as_int(&self.base.get(KEY_OCTAVES).unwrap());
                let lacunarity = if self.base.has(KEY_LACUNARITY) {
                    JsonLoaderBase::get_as_double(&self.base.get(KEY_LACUNARITY).unwrap())
                } else {
                    DEFAULT_LACUNARITY
                };
                let persistence = if self.base.has(KEY_PERSISTENCE) {
                    JsonLoaderBase::get_as_double(&self.base.get(KEY_PERSISTENCE).unwrap())
                } else {
                    DEFAULT_PERSISTENCE
                };
                noise_property = Rc::new(FractalNoiseProperty::new(
                    self.load_seed(),
                    noise_function,
                    fractal_mode,
                    octaves,
                    lacunarity,
                    persistence,
                ));
            } else {
                noise_property =
                    Rc::new(SingleNoiseProperty::new(self.load_seed(), noise_function));
            }
        }

        let type_ = property_type;
        if type_ != Some(NoisePropertyType::Formula) && self.base.has(KEY_FORMULA) {
            let formula = Self::noise_formula_value_of(&JsonLoaderBase::get_as_string(
                &self.base.get(KEY_FORMULA).unwrap(),
            ));
            noise_property = Rc::new(NoiseFormulaProperty::new(noise_property, Rc::new(formula)));
        }
        if type_ != Some(NoisePropertyType::Curve) && self.base.has(KEY_CURVE) {
            noise_property = Rc::new(
                CurveNoisePropertyJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.get(KEY_CURVE),
                    Some(noise_property.clone()),
                )
                .load(),
            );
        }
        if type_ != Some(NoisePropertyType::Scale) && self.base.has(KEY_SCALE) {
            let scale = JsonLoaderBase::get_as_double(&self.base.get(KEY_SCALE).unwrap());
            noise_property = Rc::new(ScaleNoiseProperty::uniform(noise_property, scale));
        }
        // Note: the gradient container has a "Normalize" field, so we filter
        // its type out here when checking for the regular "Normalize"
        // noise-property.
        if type_ != Some(NoisePropertyType::Normalize)
            && self.base.has(KEY_NORMALIZE)
            && type_ != Some(NoisePropertyType::Gradient)
        {
            let range = DoubleRangeJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(KEY_NORMALIZE),
            )
            .load();
            use crate::noise::supplier::IDoubleRange;
            noise_property = Rc::new(NormalizeNoiseProperty::new(
                noise_property,
                range.get_value(0.0),
                range.get_value(1.0) - range.get_value(0.0),
            ));
        }
        if type_ != Some(NoisePropertyType::Offset)
            && (self.base.has(KEY_OFFSET)
                || self.base.has(KEY_OFFSET_X)
                || self.base.has(KEY_OFFSET_Y)
                || self.base.has(KEY_OFFSET_Z))
        {
            let offset = if self.base.has(KEY_OFFSET) {
                JsonLoaderBase::get_as_double(&self.base.get(KEY_OFFSET).unwrap())
            } else {
                0.0
            };
            let offset_x = if self.base.has(KEY_OFFSET_X) {
                JsonLoaderBase::get_as_double(&self.base.get(KEY_OFFSET_X).unwrap())
            } else {
                offset
            };
            let offset_y = if self.base.has(KEY_OFFSET_Y) {
                JsonLoaderBase::get_as_double(&self.base.get(KEY_OFFSET_Y).unwrap())
            } else {
                offset
            };
            let offset_z = if self.base.has(KEY_OFFSET_Z) {
                JsonLoaderBase::get_as_double(&self.base.get(KEY_OFFSET_Z).unwrap())
            } else {
                offset
            };
            noise_property = Rc::new(OffsetNoiseProperty::new(
                noise_property,
                offset_x,
                offset_y,
                offset_z,
            ));
        }
        if type_ != Some(NoisePropertyType::Rotate)
            && (self.base.has(KEY_PITCH) || self.base.has(KEY_YAW))
        {
            let rotation = CoordinateRotatorJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.json.clone(),
            )
            .load();
            noise_property = Rc::new(RotateNoiseProperty::with_rotator(
                noise_property,
                rotation.into_matrix_rotator(),
            ));
        }
        if type_ != Some(NoisePropertyType::Gradient) && self.base.has(KEY_GRADIENT) {
            noise_property = Rc::new(
                GradientNoisePropertyJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.get(KEY_GRADIENT),
                    noise_property.clone(),
                )
                .load(),
            );
        }
        noise_property
    }

    /// Java `NoisePropertyType.valueOf(String)` — exact enum names.
    pub fn noise_property_type_value_of(name: &str) -> NoisePropertyType {
        match name {
            "MAX" => NoisePropertyType::Max,
            "MIN" => NoisePropertyType::Min,
            "SUM" => NoisePropertyType::Sum,
            "SCALE" => NoisePropertyType::Scale,
            "FORMULA" => NoisePropertyType::Formula,
            "MULTIPLY" => NoisePropertyType::Multiply,
            "DISTORTED" => NoisePropertyType::Distorted,
            "NORMALIZE" => NoisePropertyType::Normalize,
            "INVERT" => NoisePropertyType::Invert,
            "OFFSET" => NoisePropertyType::Offset,
            "ROTATE" => NoisePropertyType::Rotate,
            "GRADIENT" => NoisePropertyType::Gradient,
            "CURVE" => NoisePropertyType::Curve,
            "BLEND" => NoisePropertyType::Blend,
            other => panic!("No enum constant NoisePropertyType.{other}"),
        }
    }

    /// Java `FractalNoiseProperty.FractalMode.valueOf(String)`.
    pub fn fractal_mode_value_of(name: &str) -> FractalMode {
        match name {
            "FBM" => FractalMode::Fbm,
            "BILLOW" => FractalMode::Billow,
            "MULTI_RIGID" => FractalMode::MultiRigid,
            "OLDSCHOOL" => FractalMode::Oldschool,
            "MIN" => FractalMode::Min,
            "MAX" => FractalMode::Max,
            other => panic!("No enum constant FractalNoiseProperty.FractalMode.{other}"),
        }
    }

    /// Java `NoiseFormulaProperty.NoiseFormula.valueOf(String)`.
    pub fn noise_formula_value_of(name: &str) -> NoiseFormula {
        match name {
            "NORMAL" => NoiseFormula::Normal,
            "INVERTED" => NoiseFormula::Inverted,
            "SQUARED" => NoiseFormula::Squared,
            "INVERTED_SQUARED" => NoiseFormula::InvertedSquared,
            "SQRT" => NoiseFormula::Sqrt,
            "INVERTED_SQRT" => NoiseFormula::InvertedSqrt,
            "RIDGED" => NoiseFormula::Ridged,
            "INVERTED_RIDGED" => NoiseFormula::InvertedRidged,
            "RIDGED_SQRT" => NoiseFormula::RidgedSqrt,
            "INVERTED_RIDGED_SQRT" => NoiseFormula::InvertedRidgedSqrt,
            "RIDGED_FIX" => NoiseFormula::RidgedFix,
            "INVERTED_RIDGED_FIX" => NoiseFormula::InvertedRidgedFix,
            "RIDGED_SQRT_FIX" => NoiseFormula::RidgedSqrtFix,
            "INVERTED_RIDGED_SQRT_FIX" => NoiseFormula::InvertedRidgedSqrtFix,
            "RIDGED_SQUARED_FIX" => NoiseFormula::RidgedSquaredFix,
            "INVERTED_RIDGED_SQUARED_FIX" => NoiseFormula::InvertedRidgedSquaredFix,
            other => panic!("No enum constant NoiseFormulaProperty.NoiseFormula.{other}"),
        }
    }

    /// Java `int loadSeed()` — the `Seed` keyword overrides the seed-string
    /// tree with `appendToOriginal`.
    pub fn load_seed(&self) -> i32 {
        const KEY_SEED: &str = "Seed";
        let mut seed_val = self.base.seed.hash_code();
        if self.base.has(KEY_SEED) {
            let overwritten = self
                .base
                .seed
                .append_to_original(&JsonLoaderBase::get_as_string(
                    &self.base.get(KEY_SEED).unwrap(),
                ));
            seed_val = overwritten.hash_code();
        }
        seed_val
    }

    /// Java `NoiseProperty[] loadNoiseProperties(JsonElement element)`.
    pub fn load_noise_properties(&self, element: &Value) -> Vec<Rc<dyn NoiseProperty>> {
        if element.is_array() {
            let array = element.as_array().unwrap();
            let mut noise_properties = Vec::with_capacity(array.len());
            for (i, item) in array.iter().enumerate() {
                noise_properties.push(
                    NoisePropertyJsonLoader::new(
                        self.base.seed.append(&format!("-#{i}")),
                        &self.base.data_folder,
                        Some(item.clone()),
                    )
                    .load(),
                );
            }
            noise_properties
        } else {
            Vec::new()
        }
    }

    /// Java `double[] loadDoubleArray(JsonElement element, int size)` — a
    /// null element fills with `1.0 / size`.
    pub fn load_double_array(&self, element: Option<Value>, size: usize) -> Vec<f64> {
        let mut values = vec![0.0; size];
        match element {
            None | Some(Value::Null) => {
                values.fill(1.0 / size as f64);
            }
            Some(Value::Array(array)) => {
                for (i, value) in values.iter_mut().enumerate() {
                    *value = JsonLoaderBase::get_as_double(&array[i]);
                }
            }
            _ => {}
        }
        values
    }
}
