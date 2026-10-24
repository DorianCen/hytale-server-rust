//! P1 block/fluid/audio child-store packet encoders.
//!
//! These stores are not part of the P0 config packet order yet; the encoders are
//! exposed on [`AssetRegistry`] so the T39 config wiring and the world/network
//! layers can send them. Every field mirrors the Java `toPacket()` of the
//! corresponding config class, including its defaults and `NOT_FOUND`
//! (`Integer.MIN_VALUE`) sentinels.

use std::collections::BTreeMap;

use hytale_protocol::generated::assets::{
    UpdateBlockBreakingDecals, UpdateBlockParticleSets, UpdateBlockSoundSets, UpdateFluidFX,
    UpdateSoundEvents,
};
use hytale_protocol::generated::common::{
    AudioCategoryDuckingRule, BarBeatDuration, BlockBreakingDecal, BlockParticleEvent,
    BlockParticleSet, BlockSoundEvent, BlockSoundSet, Direction, FadeCurve, FloatRange, FluidFX,
    FluidFXMovementSettings, FluidFog, FluidParticle, NearFar, ShaderType, SoundEvent,
    SoundEventLayer, SoundEventLayerRandomSettings, StateBinding, StateDelta, StateFade,
    UpdateType,
};
use hytale_protocol::generated::wire::Vector3f;
use serde_json::Value;

use crate::packets::{color, invalid, next_id, AssetPacket, AssetPacketError, Result};
use crate::registry::AssetRegistry;

/// Java `AssetMapWithIndexes.NOT_FOUND`.
const NOT_FOUND: i32 = i32::MIN;

impl AssetRegistry {
    /// `UpdateBlockBreakingDecals` (id 45) from the `BlockBreakingDecal` store.
    pub fn block_breaking_decal_packet(&self) -> Result<AssetPacket> {
        let mut decals = BTreeMap::new();
        for record in self.block_breaking_decals.iter() {
            let stage_textures = record
                .value
                .get("StageTextures")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            decals.insert(record.key.clone(), BlockBreakingDecal { stage_textures });
        }
        let packet = UpdateBlockBreakingDecals {
            r#type: UpdateType::Init,
            block_breaking_decals: decals,
        };
        AssetPacket::encode("BlockBreakingDecal", &packet)
    }

    /// `UpdateBlockParticleSets` (id 44) from the `BlockParticleSet` store.
    pub fn block_particle_set_packet(&self) -> Result<AssetPacket> {
        let mut sets = BTreeMap::new();
        for record in self.block_particle_sets.iter() {
            let json = &record.value;
            let particles = match json.get("Particles") {
                None | Some(Value::Null) => None,
                Some(value) => {
                    let map = value.as_object().ok_or_else(|| {
                        invalid(
                            "BlockParticleSet",
                            &record.key,
                            "Particles must be an object",
                        )
                    })?;
                    let mut parsed = BTreeMap::new();
                    for (event, system) in map {
                        let event = particle_event(event).ok_or_else(|| {
                            invalid(
                                "BlockParticleSet",
                                &record.key,
                                format!("unknown BlockParticleEvent `{event}`"),
                            )
                        })?;
                        let system = system.as_str().ok_or_else(|| {
                            invalid(
                                "BlockParticleSet",
                                &record.key,
                                "particle system ids must be strings",
                            )
                        })?;
                        parsed.insert(event, system.to_string());
                    }
                    Some(parsed)
                }
            };
            sets.insert(
                record.key.clone(),
                BlockParticleSet {
                    id: Some(record.key.clone()),
                    color: color(json.get("Color")),
                    scale: f32_field(json, "Scale", 1.0),
                    position_offset: json
                        .get("PositionOffset")
                        .filter(|value| !value.is_null())
                        .map(|value| vector3f(Some(value))),
                    rotation_offset: json
                        .get("RotationOffset")
                        .filter(|value| !value.is_null())
                        .map(|value| direction(Some(value))),
                    particle_system_ids: particles,
                },
            );
        }
        let packet = UpdateBlockParticleSets {
            r#type: UpdateType::Init,
            block_particle_sets: Some(sets),
        };
        AssetPacket::encode("BlockParticleSet", &packet)
    }

    /// `UpdateBlockSoundSets` (id 42) from the `BlockSoundSet` store.
    pub fn block_sound_set_packet(&self) -> Result<AssetPacket> {
        let mut sets = BTreeMap::new();
        for record in self.block_sound_sets.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "BlockSoundSet",
                key: record.key.clone(),
            })?;
            let json = &record.value;

            // Java initializes `soundEventIds` to an empty map and always
            // serializes the resolved `soundEventIndices` map (empty, never
            // null).
            let mut indices = BTreeMap::new();
            if let Some(events) = json.get("SoundEvents").and_then(Value::as_object) {
                for (event, sound) in events {
                    let event = sound_event_name(event).ok_or_else(|| {
                        invalid(
                            "BlockSoundSet",
                            &record.key,
                            format!("unknown BlockSoundEvent `{event}`"),
                        )
                    })?;
                    let sound = sound.as_str().ok_or_else(|| {
                        invalid(
                            "BlockSoundSet",
                            &record.key,
                            "SoundEvents values must be strings",
                        )
                    })?;
                    indices.insert(event, self.sound_events.id_of(sound).unwrap_or(NOT_FOUND));
                }
            }

            let range = json
                .get("MoveInRepeatRange")
                .and_then(Value::as_array)
                .filter(|values| values.len() >= 2)
                .map(|values| FloatRange {
                    inclusive_min: values[0].as_f64().unwrap_or(0.5) as f32,
                    inclusive_max: values[1].as_f64().unwrap_or(1.5) as f32,
                })
                .unwrap_or(FloatRange {
                    inclusive_min: 0.5,
                    inclusive_max: 1.5,
                });

            sets.insert(
                id,
                BlockSoundSet {
                    // The reserved `EMPTY` slot is replaced by the pack's
                    // `Empty.json`; Java keeps the map key but the asset's own
                    // id comes from the file name.
                    id: Some(asset_id(
                        &record.key,
                        record.source.as_str(),
                        record.preloaded,
                    )),
                    sound_event_indices: Some(indices),
                    move_in_repeat_range: Some(range),
                },
            );
        }
        let packet = UpdateBlockSoundSets {
            r#type: UpdateType::Init,
            max_id: next_id(&self.block_sound_sets, "BlockSoundSet")?,
            block_sound_sets: Some(sets),
        };
        AssetPacket::encode("BlockSoundSet", &packet)
    }

    /// `UpdateFluidFX` (id 63) from the `FluidFX` store.
    pub fn fluid_fx_packet(&self) -> Result<AssetPacket> {
        let mut effects = BTreeMap::new();
        for record in self.fluid_fx.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "FluidFX",
                key: record.key.clone(),
            })?;
            effects.insert(id, fluid_fx_value(&record.key, &record.value)?);
        }
        let packet = UpdateFluidFX {
            r#type: UpdateType::Init,
            max_id: next_id(&self.fluid_fx, "FluidFX")?,
            fluid_fx: Some(effects),
        };
        AssetPacket::encode("FluidFX", &packet)
    }

    /// `UpdateSoundEvents` (id 65) from the `SoundEvent` store.
    pub fn sound_event_packet(&self) -> Result<AssetPacket> {
        let mut events = BTreeMap::new();
        for record in self.sound_events.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "SoundEvent",
                key: record.key.clone(),
            })?;
            events.insert(id, self.sound_event_value(&record.key, &record.value)?);
        }
        let packet = UpdateSoundEvents {
            r#type: UpdateType::Init,
            max_id: next_id(&self.sound_events, "SoundEvent")?,
            sound_events: Some(events),
        };
        AssetPacket::encode("SoundEvent", &packet)
    }

    /// Encode one Java `SoundEvent.toPacket()` value.
    fn sound_event_value(&self, key: &str, json: &Value) -> Result<SoundEvent> {
        let audio_category = json
            .get("AudioCategory")
            .and_then(Value::as_str)
            .map(|category| self.audio_categories.id_of(category).unwrap_or(NOT_FOUND))
            .unwrap_or(0);

        let layers = match json.get("Layers").and_then(Value::as_array) {
            Some(values) if !values.is_empty() => {
                let parsed = values
                    .iter()
                    .map(sound_event_layer)
                    .collect::<Result<Vec<_>>>()?;
                Some(parsed)
            }
            _ => None,
        };

        let state_bindings = self.state_bindings(key, json.get("StateBindings"))?;
        let ducking_rules = self.ducking_rules(json.get("DuckingRules"))?;

        Ok(SoundEvent {
            id: Some(key.to_string()),
            // The Java field defaults are linear gains (1.0); authored values
            // are decibels/semitones and are converted.
            volume: json
                .get("Volume")
                .and_then(Value::as_f64)
                .map_or(1.0, |value| decibels_to_linear(value as f32)),
            pitch: json
                .get("Pitch")
                .and_then(Value::as_f64)
                .map_or(1.0, |value| semitones_to_pitch(value as f32)),
            max_instance: i32_field(json, "MaxInstance", 50),
            prevent_sound_interruption: bool_field(json, "PreventSoundInterruption", false),
            start_attenuation_distance: f32_field(json, "StartAttenuationDistance", 2.0),
            max_distance: f32_field(json, "MaxDistance", 16.0),
            spatial_blend: f32_field(json, "SpatialBlend", 0.6),
            layers,
            audio_category,
            state_bindings,
            ducking_rules,
            bypass_ducking: bool_field(json, "BypassDucking", false),
        })
    }

    /// Java `AudioStateResolver.resolveBindings` + `StateBindingConfig.toPacket`.
    fn state_bindings(
        &self,
        key: &str,
        value: Option<&Value>,
    ) -> Result<Option<Vec<StateBinding>>> {
        let Some(entries) = value.and_then(Value::as_array).filter(|e| !e.is_empty()) else {
            return Ok(None);
        };
        let mut bindings = Vec::with_capacity(entries.len());
        for entry in entries {
            let state_id = entry.get("AudioState").and_then(Value::as_str);
            let state = state_id.and_then(|id| self.audio_states.get(id));
            let audio_state_index = state.and_then(|record| record.id).unwrap_or(-1);
            let values =
                state.and_then(|record| record.value.get("Values").and_then(Value::as_array));

            let deltas = match entry.get("Deltas") {
                None | Some(Value::Null) => None,
                Some(value) => {
                    let entries = value.as_array().ok_or_else(|| {
                        invalid("SoundEvent", key, "StateBindings.Deltas must be an array")
                    })?;
                    let mut parsed = Vec::with_capacity(entries.len());
                    for delta in entries {
                        let value_name = delta.get("Value").and_then(Value::as_str);
                        let value_index = if audio_state_index < 0 {
                            -1
                        } else {
                            resolve_value_index(value_name, values)
                        };
                        parsed.push(StateDelta {
                            value_index,
                            volume_db: f32_field(delta, "VolumeDb", 0.0),
                            mute: bool_field(delta, "Mute", false),
                            pitch_semitones: f32_field(delta, "PitchSemitones", 0.0),
                            fade: state_fade(delta.get("Fade"))?,
                        });
                    }
                    Some(parsed)
                }
            };

            bindings.push(StateBinding {
                audio_state_index,
                deltas,
            });
        }
        Ok(Some(bindings))
    }

    fn ducking_rules(
        &self,
        value: Option<&Value>,
    ) -> Result<Option<Vec<AudioCategoryDuckingRule>>> {
        let Some(entries) = value.and_then(Value::as_array).filter(|e| !e.is_empty()) else {
            return Ok(None);
        };
        let mut rules = Vec::with_capacity(entries.len());
        for entry in entries {
            rules.push(AudioCategoryDuckingRule {
                // `AudioCategoryDuckingRuleConfig.resolveTargets` is run by the
                // audio plugin's `LoadedAssetsEvent`, not by the asset pipeline.
                // The canonical Java init payload therefore carries the
                // unresolved `AssetMapWithIndexes.NOT_FOUND` sentinel.
                target_audio_category_index: NOT_FOUND,
                ducking_volume_db: f32_field(entry, "DuckingVolumeDb", 0.0),
                attack_ms: f32_field(entry, "AttackMs", 0.0),
                hold_ms: f32_field(entry, "HoldMs", 0.0),
                release_ms: f32_field(entry, "ReleaseMs", 0.0),
                curve: fade_curve(entry.get("Curve")),
                release_curve: fade_curve(entry.get("ReleaseCurve")),
                priority: i32_field(entry, "Priority", 0) as i16,
            });
        }
        Ok(Some(rules))
    }
}

fn fluid_fx_value(key: &str, json: &Value) -> Result<FluidFX> {
    let fog_distance = json
        .get("FogDistance")
        .and_then(Value::as_array)
        .filter(|values| values.len() >= 2)
        .map(|values| NearFar {
            near: values[0].as_f64().unwrap_or(0.0) as f32,
            far: values[1].as_f64().unwrap_or(32.0) as f32,
        })
        .unwrap_or(NearFar {
            near: 0.0,
            far: 32.0,
        });

    let colors_filter = json
        .get("ColorsFilter")
        .and_then(Value::as_array)
        .filter(|values| values.len() >= 3);
    let filter = |index: usize, default: f32| -> f32 {
        colors_filter
            .and_then(|values| values.get(index))
            .and_then(Value::as_f64)
            .map_or(default, |value| value as f32)
    };

    Ok(FluidFX {
        id: Some(key.to_string()),
        shader: ShaderType::None,
        fog_mode: fluid_fog(json.get("Fog")),
        fog_color: Some(color(json.get("FogColor")).unwrap_or(
            hytale_protocol::generated::common::Color {
                red: -1,
                green: -1,
                blue: -1,
            },
        )),
        fog_distance: Some(fog_distance),
        fog_depth_start: f32_field(json, "FogDepthStart", 40.0),
        fog_depth_falloff: f32_field(json, "FogDepthFalloff", 10.0),
        color_filter: Some(hytale_protocol::generated::common::Color {
            red: color_byte(filter(0, 1.0)),
            green: color_byte(filter(1, 1.0)),
            blue: color_byte(filter(2, 1.0)),
        }),
        color_saturation: f32_field(json, "ColorsSaturation", 1.0),
        distortion_amplitude: f32_field(json, "DistortionAmplitude", 8.0),
        distortion_frequency: f32_field(json, "DistortionFrequency", 4.0),
        particle: json
            .get("Particle")
            .filter(|value| !value.is_null())
            .map(fluid_particle)
            .transpose()?,
        movement_settings: json
            .get("MovementSettings")
            .filter(|value| !value.is_null())
            .map(movement_settings),
    })
}

pub(super) fn fluid_particle(value: &Value) -> Result<FluidParticle> {
    let system_id = value
        .get("SystemId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok(FluidParticle {
        system_id,
        color: color(value.get("Color")),
        scale: f32_field(value, "Scale", 1.0),
    })
}

fn movement_settings(value: &Value) -> FluidFXMovementSettings {
    FluidFXMovementSettings {
        swim_up_speed: f32_field(value, "SwimUpSpeed", 0.0),
        swim_down_speed: f32_field(value, "SwimDownSpeed", 0.0),
        sink_speed: f32_field(value, "SinkSpeed", 0.0),
        horizontal_speed_multiplier: f32_field(value, "HorizontalSpeedMultiplier", 0.0),
        field_of_view_multiplier: f32_field(value, "FieldOfViewMultiplier", 0.0),
        entry_velocity_multiplier: f32_field(value, "EntryVelocityMultiplier", 0.0),
    }
}

fn sound_event_layer(value: &Value) -> Result<SoundEventLayer> {
    let files = value
        .get("Files")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let random = value.get("RandomSettings").filter(|value| !value.is_null());
    let random_gain = |name: &str| -> f32 {
        random
            .and_then(|json| json.get(name))
            .and_then(Value::as_f64)
            .map_or(1.0, |value| decibels_to_linear(value as f32))
    };
    let random_pitch = |name: &str| -> f32 {
        random
            .and_then(|json| json.get(name))
            .and_then(Value::as_f64)
            .map_or(1.0, |value| semitones_to_pitch(value as f32))
    };

    Ok(SoundEventLayer {
        volume: value
            .get("Volume")
            .and_then(Value::as_f64)
            .map_or(1.0, |value| decibels_to_linear(value as f32)),
        start_delay: f32_field(value, "StartDelay", 0.0),
        fade_in: f32_field(value, "FadeIn", 0.0),
        looping: bool_field(value, "Looping", false),
        probability: i32_field(value, "Probability", 100),
        probability_reroll_delay: f32_field(value, "ProbabilityRerollDelay", 1.0),
        round_robin_history_size: i32_field(value, "RoundRobinHistorySize", 0),
        random_settings: SoundEventLayerRandomSettings {
            min_volume: random_gain("MinVolume"),
            max_volume: random_gain("MaxVolume"),
            min_pitch: random_pitch("MinPitch"),
            max_pitch: random_pitch("MaxPitch"),
            max_start_offset: random
                .and_then(|json| json.get("MaxStartOffset"))
                .and_then(Value::as_f64)
                .map_or(0.0, |value| value as f32),
        },
        files,
    })
}

fn state_fade(value: Option<&Value>) -> Result<Option<StateFade>> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    Ok(Some(StateFade {
        duration: bar_beat_duration(value.get("Duration")),
        curve: fade_curve(value.get("Curve")),
        delay: bar_beat_duration(value.get("Delay")),
    }))
}

fn bar_beat_duration(value: Option<&Value>) -> Option<BarBeatDuration> {
    let value = value.filter(|value| !value.is_null())?;
    Some(BarBeatDuration {
        bars: value.get("Bars").and_then(Value::as_i64).unwrap_or(0) as i32,
        beats: value.get("Beats").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        ms: value.get("Ms").and_then(Value::as_f64).unwrap_or(0.0) as f32,
    })
}

fn resolve_value_index(name: Option<&str>, values: Option<&Vec<Value>>) -> i32 {
    let Some(name) = name else { return -1 };
    if name == "*" {
        return -1;
    }
    let Some(values) = values else { return -1 };
    values
        .iter()
        .position(|value| value.as_str() == Some(name))
        .map_or(-1, |index| index as i32)
}

fn fluid_fog(value: Option<&Value>) -> FluidFog {
    match value.and_then(Value::as_str) {
        Some("ColorLight") => FluidFog::ColorLight,
        Some("EnvironmentTint") => FluidFog::EnvironmentTint,
        _ => FluidFog::Color,
    }
}

fn fade_curve(value: Option<&Value>) -> FadeCurve {
    match value.and_then(Value::as_str) {
        Some("Logarithmic") => FadeCurve::Logarithmic,
        Some("Exponential") => FadeCurve::Exponential,
        Some("SCurve") => FadeCurve::SCurve,
        Some("EqualPowerSine") => FadeCurve::EqualPowerSine,
        _ => FadeCurve::Linear,
    }
}

fn particle_event(name: &str) -> Option<BlockParticleEvent> {
    Some(match name {
        "Walk" => BlockParticleEvent::Walk,
        "Run" => BlockParticleEvent::Run,
        "Sprint" => BlockParticleEvent::Sprint,
        "SoftLand" => BlockParticleEvent::SoftLand,
        "HardLand" => BlockParticleEvent::HardLand,
        "MoveOut" => BlockParticleEvent::MoveOut,
        "Hit" => BlockParticleEvent::Hit,
        "Break" => BlockParticleEvent::Break,
        "Build" => BlockParticleEvent::Build,
        "Physics" => BlockParticleEvent::Physics,
        _ => return None,
    })
}

fn sound_event_name(name: &str) -> Option<BlockSoundEvent> {
    Some(match name {
        "Walk" => BlockSoundEvent::Walk,
        "Land" => BlockSoundEvent::Land,
        "MoveIn" => BlockSoundEvent::MoveIn,
        "MoveOut" => BlockSoundEvent::MoveOut,
        "Hit" => BlockSoundEvent::Hit,
        "Break" => BlockSoundEvent::Break,
        "Build" => BlockSoundEvent::Build,
        "Clone" => BlockSoundEvent::Clone,
        "Harvest" => BlockSoundEvent::Harvest,
        _ => return None,
    })
}

fn vector3f(value: Option<&Value>) -> Vector3f {
    let component = |name: &str| -> f32 {
        value
            .and_then(|value| value.get(name))
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32
    };
    Vector3f {
        x: component("X"),
        y: component("Y"),
        z: component("Z"),
    }
}

fn direction(value: Option<&Value>) -> Direction {
    let component = |name: &str| -> f32 {
        value
            .and_then(|value| value.get(name))
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32
    };
    Direction {
        yaw: component("Yaw"),
        pitch: component("Pitch"),
        roll: component("Roll"),
    }
}

fn asset_id(key: &str, source: &str, preloaded: bool) -> String {
    if preloaded {
        return key.to_string();
    }
    source
        .rsplit('/')
        .next()
        .unwrap_or(source)
        .strip_suffix(".json")
        .unwrap_or(source)
        .to_string()
}

fn f32_field(json: &Value, name: &str, default: f32) -> f32 {
    json.get(name)
        .and_then(Value::as_f64)
        .map_or(default, |value| value as f32)
}

fn i32_field(json: &Value, name: &str, default: i32) -> i32 {
    json.get(name)
        .and_then(Value::as_i64)
        .map_or(default, |value| value as i32)
}

fn bool_field(json: &Value, name: &str, default: bool) -> bool {
    json.get(name).and_then(Value::as_bool).unwrap_or(default)
}

/// Java `AudioUtil.decibelsToLinearGain`.
fn decibels_to_linear(decibels: f32) -> f32 {
    if decibels <= -100.0 {
        return 0.0;
    }
    // Java divides in float, then Math.pow works in double.
    10f64.powf(f64::from(decibels / 20.0)) as f32
}

/// Java `AudioUtil.semitonesToLinearPitch`.
fn semitones_to_pitch(semitones: f32) -> f32 {
    (1.0 / 2f64.powf(f64::from(-semitones / 12.0))) as f32
}

/// Java `(byte) (float)` for a 0..1 colour channel.
fn color_byte(channel: f32) -> i8 {
    ((channel * 255.0) as i32 as u8) as i8
}
