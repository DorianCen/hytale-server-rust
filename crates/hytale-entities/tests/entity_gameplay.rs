//! Java entity-gameplay fixture and real-pack checks.
//!
//! `testdata/golden/server/entity/entity-gameplay.json` is emitted by
//! `tools/java-dumper/run-entity-gameplay.sh` from the real asset pipeline. The
//! tests assert that the Rust loaders see the same ids and the same core fields.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_entities::damage::{DamageCause, DamageCauses, ResistanceCalculationType};
use hytale_entities::effects::{
    EffectValueType, EntityEffectAsset, OverlapBehavior, RemovalBehavior,
};
use hytale_entities::projectile::{
    ProjectileAsset, ProjectileBody, ProjectileConfigAsset, ProjectileEvent, ProjectileState,
    ProjectileTarget, RotationMode,
};
use hytale_entities::stats::{
    CalculationType, EntityStatMap, EntityStatType, ModifierTarget, RegenType, StatResetBehavior,
};
use hytale_world::collision::{Aabb, CollisionWorld, Vec3};
use serde_json::Value;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn registry() -> Option<AssetRegistry> {
    let pack = repo_root().join("Assets/Assets.zip");
    if !pack.is_file() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return None;
    }
    let pack = AssetPack::open(pack).expect("open the pack");
    Some(AssetRegistry::load_p0(&pack).expect("load P0"))
}

fn fixture() -> Value {
    let path = repo_root().join("testdata/golden/server/entity/entity-gameplay.json");
    let bytes = std::fs::read(&path).expect("read entity gameplay fixture");
    serde_json::from_slice(&bytes).expect("parse entity gameplay fixture")
}

fn f32_at(value: &Value, key: &str) -> f32 {
    value.get(key).and_then(Value::as_f64).unwrap_or(0.0) as f32
}

fn assert_f32(actual: f32, expected: f32, context: &str) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "{context}: expected {expected}, got {actual}"
    );
}

fn f64_at(value: &Value, key: &str) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn assert_f64(actual: f64, expected: f64, context: &str) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "{context}: expected {expected}, got {actual}"
    );
}

#[test]
fn entity_stat_ids_and_fields_match_the_java_fixture() {
    let Some(registry) = registry() else {
        return;
    };
    let fixture = fixture();
    let stats = fixture["stats"].as_object().expect("stats object");
    assert_eq!(
        registry.entity_stat_types.len(),
        stats.len(),
        "EntityStatType store count"
    );

    for (key, expected) in stats {
        let index = expected["index"].as_i64().unwrap() as i32;
        assert_eq!(
            registry.entity_stat_id(key),
            Some(index),
            "EntityStatType `{key}` id"
        );
        let record = registry
            .entity_stat_types
            .get(key)
            .unwrap_or_else(|| panic!("missing EntityStatType `{key}`"));
        assert_eq!(record.id, Some(index), "record id for `{key}`");
        let stat = EntityStatType::from_value(key, index, &record.value).expect("decode stat");

        assert_eq!(stat.id, expected["id"].as_str().unwrap());
        assert_f32(stat.initial_value, f32_at(expected, "initialValue"), key);
        assert_f32(stat.min, f32_at(expected, "min"), key);
        assert_f32(stat.max, f32_at(expected, "max"), key);
        assert_eq!(stat.shared, expected["shared"].as_bool().unwrap(), "{key}");
        assert_eq!(
            stat.reset_behavior,
            match expected["resetType"].as_str().unwrap() {
                "MaxValue" => StatResetBehavior::MaxValue,
                _ => StatResetBehavior::InitialValue,
            },
            "{key} reset"
        );
        assert_eq!(
            stat.ignore_invulnerability,
            expected["ignoreInvulnerability"].as_bool().unwrap(),
            "{key} ignore invulnerability"
        );
        assert_eq!(
            stat.hide_from_tooltip,
            expected["hideFromTooltip"].as_bool().unwrap(),
            "{key} hide from tooltip"
        );
        let rules = expected["regenerating"].as_array().unwrap();
        assert_eq!(stat.regenerating.len(), rules.len(), "{key} regen count");
        for (rule, expected) in stat.regenerating.iter().zip(rules) {
            assert_f32(rule.interval, f32_at(expected, "interval"), key);
            assert_f32(rule.amount, f32_at(expected, "amount"), key);
            assert_eq!(
                rule.clamp_at_zero,
                expected["clampAtZero"].as_bool().unwrap(),
                "{key} clampAtZero"
            );
            assert_eq!(
                rule.regen_type,
                match expected["regenType"].as_str().unwrap() {
                    "Percentage" | "PERCENTAGE" => RegenType::Percentage,
                    _ => RegenType::Additive,
                },
                "{key} regen type"
            );
            assert_eq!(
                rule.conditions.len(),
                expected["conditionCount"].as_u64().unwrap() as usize,
                "{key} condition count"
            );
        }
    }
}

#[test]
fn entity_effect_ids_and_fields_match_the_java_fixture() {
    let Some(registry) = registry() else {
        return;
    };
    let fixture = fixture();
    let effects = fixture["effects"].as_object().expect("effects object");
    assert_eq!(
        registry.entity_effects.len(),
        effects.len(),
        "EntityEffect store count"
    );

    for (key, expected) in effects {
        let index = expected["index"].as_i64().unwrap() as i32;
        assert_eq!(
            registry.entity_effect_id(key),
            Some(index),
            "EntityEffect `{key}` id"
        );
        let record = registry
            .entity_effects
            .get(key)
            .unwrap_or_else(|| panic!("missing EntityEffect `{key}`"));
        let effect = EntityEffectAsset::from_value(key, index, &record.value, |name| {
            registry.entity_stat_id(name)
        })
        .unwrap_or_else(|error| panic!("decode `{key}`: {error}"));

        assert_eq!(effect.id, expected["id"].as_str().unwrap());
        assert_eq!(effect.name.as_deref(), expected["name"].as_str());
        assert_f32(effect.duration, f32_at(expected, "duration"), key);
        assert_eq!(
            effect.overlap_behavior,
            match expected["overlapBehavior"].as_str().unwrap() {
                "Overwrite" | "OVERWRITE" => OverlapBehavior::Overwrite,
                "Extend" | "EXTEND" => OverlapBehavior::Extend,
                _ => OverlapBehavior::Ignore,
            },
            "{key} overlap"
        );
        assert_eq!(
            effect.removal_behavior,
            match expected["removalBehavior"].as_str().unwrap() {
                "Infinite" | "INFINITE" => RemovalBehavior::Infinite,
                "Duration" | "DURATION" => RemovalBehavior::Duration,
                _ => RemovalBehavior::Complete,
            },
            "{key} removal"
        );
        assert_eq!(
            effect.infinite,
            expected["infinite"].as_bool().unwrap(),
            "{key} infinite"
        );
        assert_eq!(
            effect.debuff,
            expected["debuff"].as_bool().unwrap(),
            "{key} debuff"
        );
        assert_eq!(
            effect.status_effect_icon.as_deref(),
            expected["statusEffectIcon"].as_str(),
            "{key} icon"
        );
        assert_eq!(
            effect.invulnerable,
            expected["invulnerable"].as_bool().unwrap(),
            "{key} invulnerable"
        );
        assert_eq!(
            effect.value_type,
            match expected["valueType"].as_str().unwrap() {
                "Percent" => EffectValueType::Percent,
                _ => EffectValueType::Absolute,
            },
            "{key} value type"
        );
        assert_f32(
            effect.damage_calculator_cooldown,
            f32_at(expected, "damageCalculatorCooldown"),
            key,
        );
        assert_eq!(
            effect.damage_calculator.is_some(),
            expected["hasDamageCalculator"].as_bool().unwrap(),
            "{key} damage calculator"
        );
        assert_eq!(
            effect.apply_conditions.len(),
            expected["applyConditionCount"].as_u64().unwrap() as usize,
            "{key} apply conditions"
        );

        let expected_stats = expected["entityStats"].as_object().unwrap();
        assert_eq!(effect.entity_stats.len(), expected_stats.len(), "{key}");
        for (name, amount) in expected_stats {
            let id = registry.entity_stat_id(name).unwrap_or_else(|| {
                panic!("fixture effect `{key}` references unknown stat `{name}`")
            });
            assert_f32(
                effect.entity_stats[&id],
                amount.as_f64().unwrap() as f32,
                &format!("{key} stat {name}"),
            );
        }

        let expected_modifiers = expected["statModifiers"].as_object().unwrap();
        assert_eq!(
            effect.stat_modifiers.len(),
            expected_modifiers.len(),
            "{key} stat modifier count"
        );
        for (name, modifiers) in expected_modifiers {
            let id = registry.entity_stat_id(name).unwrap();
            let actual = effect.stat_modifiers.get(&id).expect("stat modifiers");
            let expected = modifiers.as_array().unwrap();
            assert_eq!(actual.len(), expected.len(), "{key} {name} modifier count");
            for (modifier, expected) in actual.iter().zip(expected) {
                assert_eq!(
                    modifier.target,
                    match expected["target"].as_str().unwrap() {
                        "Min" | "MIN" => ModifierTarget::Min,
                        _ => ModifierTarget::Max,
                    },
                    "{key} {name} target"
                );
                assert_eq!(
                    modifier.calculation,
                    match expected["calculationType"].as_str().unwrap() {
                        "Multiplicative" | "MULTIPLICATIVE" => CalculationType::Multiplicative,
                        _ => CalculationType::Additive,
                    },
                    "{key} {name} calculation"
                );
                assert_f32(
                    modifier.amount,
                    f32_at(expected, "amount"),
                    &format!("{key} {name}"),
                );
            }
        }

        assert_resistance(
            &effect.damage_resistance,
            expected["damageResistance"].as_object().unwrap(),
            key,
        );
        assert_resistance(
            &effect.outgoing_damage,
            expected["outgoingDamage"].as_object().unwrap(),
            key,
        );
    }
}

fn assert_resistance(
    actual: &BTreeMap<String, Vec<hytale_entities::damage::ResistanceModifier>>,
    expected: &serde_json::Map<String, Value>,
    context: &str,
) {
    assert_eq!(actual.len(), expected.len(), "{context} resistance count");
    for (cause, modifiers) in expected {
        let actual = actual
            .get(cause)
            .unwrap_or_else(|| panic!("{context}: missing resistance `{cause}`"));
        let expected = modifiers.as_array().unwrap();
        assert_eq!(
            actual.len(),
            expected.len(),
            "{context} {cause} modifier count"
        );
        for (modifier, expected) in actual.iter().zip(expected) {
            assert_eq!(
                modifier.calculation_type,
                match expected["calculationType"].as_str().unwrap() {
                    "Percent" | "PERCENT" => ResistanceCalculationType::Percent,
                    _ => ResistanceCalculationType::Flat,
                },
                "{context} {cause} calculation"
            );
            assert_f32(
                modifier.amount,
                f32_at(expected, "amount"),
                &format!("{context} {cause}"),
            );
        }
    }
}

#[test]
fn damage_cause_ids_and_fields_match_the_java_fixture() {
    let Some(registry) = registry() else {
        return;
    };
    let fixture = fixture();
    let causes = fixture["damageCauses"]
        .as_object()
        .expect("damageCauses object");
    assert_eq!(
        registry.damage_causes.len(),
        causes.len(),
        "DamageCause store count"
    );

    let mut decoded = Vec::new();
    for (key, expected) in causes {
        let index = expected["index"].as_i64().unwrap() as i32;
        assert_eq!(
            registry.damage_cause_id(key),
            Some(index),
            "DamageCause `{key}` id"
        );
        let record = registry
            .damage_causes
            .get(key)
            .unwrap_or_else(|| panic!("missing DamageCause `{key}`"));
        let cause = DamageCause::from_value(key, &record.value);
        assert_eq!(cause.id, expected["id"].as_str().unwrap());
        assert_eq!(
            cause.inherits.as_deref(),
            expected["inherits"].as_str(),
            "{key} inherits"
        );
        assert_eq!(
            cause.damage_text_color.as_deref(),
            expected["damageTextColor"].as_str(),
            "{key} color"
        );
        assert_eq!(
            cause.durability_loss,
            expected["durabilityLoss"].as_bool().unwrap(),
            "{key} durability"
        );
        assert_eq!(
            cause.stamina_loss,
            expected["staminaLoss"].as_bool().unwrap(),
            "{key} stamina"
        );
        assert_eq!(
            cause.bypass_resistances,
            expected["bypassResistances"].as_bool().unwrap(),
            "{key} bypass"
        );
        assert_eq!(
            cause.animation_id,
            expected["animationId"].as_str().unwrap(),
            "{key} animation"
        );
        assert_eq!(
            cause.death_animation_id,
            expected["deathAnimationId"].as_str().unwrap(),
            "{key} death animation"
        );
        assert_eq!(
            cause.death_message_key.as_deref(),
            expected["deathMessageKey"].as_str(),
            "{key} death message"
        );
        decoded.push(cause);
    }

    let causes = DamageCauses::new(decoded);
    assert!(causes.is_or_inherits("Slashing", "Physical"));
    assert!(causes.is_or_inherits("Drowning", "Environment"));
    assert!(!causes.is_or_inherits("Physical", "Slashing"));
    assert!(causes.get("Projectile").is_some());
}

#[test]
fn projectile_assets_and_configs_match_the_java_fixture() {
    let Some(registry) = registry() else {
        return;
    };
    let fixture = fixture();

    let projectiles = fixture["projectiles"].as_object().expect("projectiles");
    assert_eq!(
        registry.projectiles.len(),
        projectiles.len(),
        "Projectile store count"
    );
    for (key, expected) in projectiles {
        let value = registry
            .projectile(key)
            .unwrap_or_else(|| panic!("missing Projectile `{key}`"));
        let asset = ProjectileAsset::from_value(key, value).expect("decode projectile");
        assert_eq!(asset.id, expected["id"].as_str().unwrap());
        assert_eq!(asset.appearance.as_deref(), expected["appearance"].as_str());
        assert_f64(asset.radius, f64_at(expected, "radius"), key);
        assert_f64(asset.height, f64_at(expected, "height"), key);
        assert_f64(
            asset.vertical_center_shot,
            f64_at(expected, "verticalCenterShot"),
            key,
        );
        assert_f64(
            asset.horizontal_center_shot,
            f64_at(expected, "horizontalCenterShot"),
            key,
        );
        assert_f64(asset.depth_shot, f64_at(expected, "depthShot"), key);
        assert_eq!(
            asset.pitch_adjust_shot,
            expected["pitchAdjustShot"].as_bool().unwrap()
        );
        assert_f64(
            asset.muzzle_velocity,
            f64_at(expected, "muzzleVelocity"),
            key,
        );
        assert_f64(
            asset.terminal_velocity,
            f64_at(expected, "terminalVelocity"),
            key,
        );
        assert_f64(asset.gravity, f64_at(expected, "gravity"), key);
        assert_f64(asset.bounciness, f64_at(expected, "bounciness"), key);
        assert_f64(
            asset.impact_slowdown,
            f64_at(expected, "impactSlowdown"),
            key,
        );
        assert_eq!(
            asset.sticks_vertically,
            expected["sticksVertically"].as_bool().unwrap(),
            "{key} sticks"
        );
        assert_eq!(asset.compute_yaw, expected["computeYaw"].as_bool().unwrap());
        assert_eq!(
            asset.compute_pitch,
            expected["computePitch"].as_bool().unwrap()
        );
        assert_eq!(
            asset.compute_roll,
            expected["computeRoll"].as_bool().unwrap()
        );
        assert_eq!(
            asset.rotation_mode,
            match expected["rotationMode"].as_str().unwrap() {
                "None" => RotationMode::None,
                "Velocity" => RotationMode::Velocity,
                "VelocityRoll" => RotationMode::VelocityRoll,
                _ => RotationMode::VelocityDamped,
            },
            "{key} rotation mode"
        );
        assert_f64(asset.time_to_live, f64_at(expected, "timeToLive"), key);
        assert_eq!(asset.damage, expected["damage"].as_i64().unwrap() as i32);
        assert_f64(asset.dead_time, f64_at(expected, "deadTime"), key);
        assert_f64(asset.dead_time_miss, f64_at(expected, "deadTimeMiss"), key);
        assert_eq!(
            asset.death_effects_on_hit,
            expected["deathEffectsOnHit"].as_bool().unwrap()
        );
        assert_f64(asset.density, f64_at(expected, "density"), key);
        assert_f64(
            asset.water_terminal_velocity_multiplier,
            f64_at(expected, "waterTerminalVelocityMultiplier"),
            key,
        );
        assert_f64(
            asset.water_hit_impulse_loss,
            f64_at(expected, "waterHitImpulseLoss"),
            key,
        );
        assert_f64(
            asset.damping_rotation,
            f64_at(expected, "dampingRotation"),
            key,
        );
        assert_f64(
            asset.rotation_speed_velocity_ratio,
            f64_at(expected, "rotationSpeedVelocityRatio"),
            key,
        );
        assert_f64(
            asset.swimming_damping_factor,
            f64_at(expected, "swimmingDampingFactor"),
            key,
        );
        assert_eq!(
            asset.bounce_sound_event_id.as_deref(),
            expected["bounceSoundEventId"].as_str()
        );
        assert_eq!(
            asset.hit_sound_event_id.as_deref(),
            expected["hitSoundEventId"].as_str()
        );
        assert_eq!(
            asset.miss_sound_event_id.as_deref(),
            expected["missSoundEventId"].as_str()
        );
        assert_eq!(
            asset.death_sound_event_id.as_deref(),
            expected["deathSoundEventId"].as_str()
        );
        assert_eq!(
            asset.has_bounce_particles,
            expected["hasBounceParticles"].as_bool().unwrap()
        );
        assert_eq!(
            asset.has_hit_particles,
            expected["hasHitParticles"].as_bool().unwrap()
        );
        assert_eq!(
            asset.has_miss_particles,
            expected["hasMissParticles"].as_bool().unwrap()
        );
        assert_eq!(
            asset.has_death_particles,
            expected["hasDeathParticles"].as_bool().unwrap()
        );
        assert_eq!(
            asset.has_explosion_config,
            expected["hasExplosionConfig"].as_bool().unwrap()
        );
    }

    let configs = fixture["projectileConfigs"]
        .as_object()
        .expect("projectileConfigs");
    assert_eq!(
        registry.projectile_configs.len(),
        configs.len(),
        "ProjectileConfig store count"
    );
    for (key, expected) in configs {
        let value = registry
            .projectile_config(key)
            .unwrap_or_else(|| panic!("missing ProjectileConfig `{key}`"));
        let config = ProjectileConfigAsset::from_value(key, value).expect("decode config");
        assert_eq!(config.id, expected["id"].as_str().unwrap());
        assert_eq!(config.model.as_deref(), expected["model"].as_str());
        assert_eq!(
            config.use_model_scale,
            expected["useModelScale"].as_bool().unwrap()
        );
        assert_f64(config.launch_force, f64_at(expected, "launchForce"), key);
        assert_f64(config.spawn_offset.x, f64_at(expected, "spawnOffsetX"), key);
        assert_f64(config.spawn_offset.y, f64_at(expected, "spawnOffsetY"), key);
        assert_f64(config.spawn_offset.z, f64_at(expected, "spawnOffsetZ"), key);
        assert_eq!(
            config.rotate_spawn_offset_by_pitch,
            expected["rotateSpawnOffsetByPitch"].as_bool().unwrap()
        );
        assert_eq!(
            config.rotate_spawn_offset_by_yaw,
            expected["rotateSpawnOffsetByYaw"].as_bool().unwrap()
        );
        let physics = &expected["physics"];
        assert_eq!(
            config.physics.ground_follow,
            expected["physicsType"].as_str() == Some("GroundFollow"),
            "{key} physicsType"
        );
        assert_f64(
            config.physics.density,
            f64_at(physics, "density"),
            &format!("{key} density"),
        );
        assert_f64(
            config.physics.gravity,
            f64_at(physics, "gravity"),
            &format!("{key} gravity"),
        );
        assert_f64(
            config.physics.bounciness,
            f64_at(physics, "bounciness"),
            &format!("{key} bounciness"),
        );
        assert_eq!(
            config.physics.bounce_count,
            physics["bounceCount"].as_i64().unwrap() as i32
        );
        assert_f64(
            config.physics.bounce_limit,
            f64_at(physics, "bounceLimit"),
            key,
        );
        assert_eq!(
            config.physics.sticks_vertically,
            physics["sticksVertically"].as_bool().unwrap(),
            "{key} sticksVertically"
        );
        assert_eq!(
            config.physics.compute_yaw,
            physics["computeYaw"].as_bool().unwrap(),
            "{key} computeYaw"
        );
        assert_eq!(
            config.physics.compute_pitch,
            physics["computePitch"].as_bool().unwrap(),
            "{key} computePitch"
        );
        assert_eq!(
            config.physics.rotation_mode,
            match physics["rotationMode"].as_str().unwrap() {
                "None" => RotationMode::None,
                "Velocity" => RotationMode::Velocity,
                "VelocityRoll" => RotationMode::VelocityRoll,
                _ => RotationMode::VelocityDamped,
            }
        );
        assert_f64(
            config.physics.move_out_of_solid_speed,
            f64_at(physics, "moveOutOfSolidSpeed"),
            key,
        );
        assert_f64(
            config.physics.terminal_velocity_air,
            f64_at(physics, "terminalVelocityAir"),
            key,
        );
        assert_f64(
            config.physics.density_air,
            f64_at(physics, "densityAir"),
            key,
        );
        assert_f64(
            config.physics.terminal_velocity_water,
            f64_at(physics, "terminalVelocityWater"),
            key,
        );
        assert_f64(
            config.physics.density_water,
            f64_at(physics, "densityWater"),
            key,
        );
        assert_f64(
            config.physics.hit_water_impulse_loss,
            f64_at(physics, "hitWaterImpulseLoss"),
            key,
        );
        assert_f64(
            config.physics.rotation_force,
            f64_at(physics, "rotationForce"),
            key,
        );
        assert_f64(
            config.physics.speed_rotation_factor,
            f64_at(physics, "speedRotationFactor"),
            key,
        );
        assert_f64(
            config.physics.swimming_damping_factor,
            f64_at(physics, "swimmingDampingFactor"),
            key,
        );
        assert_eq!(
            config.physics.allow_rolling,
            physics["allowRolling"].as_bool().unwrap(),
            "{key} allowRolling"
        );
        assert_f64(
            config.physics.rolling_friction_factor,
            f64_at(physics, "rollingFrictionFactor"),
            key,
        );
        assert_f64(
            config.physics.rolling_speed,
            f64_at(physics, "rollingSpeed"),
            key,
        );
        if config.physics.ground_follow {
            assert_eq!(
                config.physics.max_climb,
                physics["maxClimb"].as_i64().unwrap() as i32
            );
            assert_eq!(
                config.physics.max_drop,
                physics["maxDrop"].as_i64().unwrap() as i32
            );
        }
    }
}

#[test]
fn real_projectile_assets_fly_hit_and_miss() {
    let Some(registry) = registry() else {
        return;
    };
    let arrow = registry
        .projectile("Arrow_NoCharge")
        .expect("Arrow_NoCharge");
    let arrow = ProjectileAsset::from_value("Arrow_NoCharge", arrow).expect("decode arrow");

    struct EmptyWorld;
    impl CollisionWorld for EmptyWorld {
        fn collect_block_boxes(&self, _x: i32, _y: i32, _z: i32, _out: &mut Vec<Aabb>) {}
    }
    let empty = EmptyWorld;
    let mut body = ProjectileBody::launch(arrow.clone(), 1, Vec3::new(0.5, 10.0, 0.5), 0.0, 0.0);
    let start = body.position;
    let step = body.step(&empty, &[], 1.0);
    assert_eq!(step.state, ProjectileState::Flying);
    assert!(step.position.z < start.z, "yaw 0 flies toward -Z");
    assert!(step.position.y < start.y, "gravity pulls it down");
    assert!(
        body.velocity.length_squared().sqrt() <= arrow.terminal_velocity + 1e-9,
        "terminal velocity caps the speed"
    );

    struct Wall(Aabb);
    impl CollisionWorld for Wall {
        fn collect_block_boxes(&self, _x: i32, _y: i32, _z: i32, out: &mut Vec<Aabb>) {
            out.push(self.0);
        }
    }
    let wall = Wall(Aabb::unit().translated(Vec3::new(0.0, 0.0, -2.0)));
    let mut body = ProjectileBody::launch(arrow, 1, Vec3::new(0.5, 1.0, 0.5), 0.0, 0.0);
    let mut saw_miss = false;
    for _ in 0..120 {
        let step = body.step(&wall, &[], 0.05);
        if step
            .events
            .iter()
            .any(|event| matches!(event, ProjectileEvent::Miss { .. }))
        {
            saw_miss = true;
            break;
        }
        if !body.is_alive() {
            break;
        }
    }
    assert!(saw_miss, "the wall hit reports a miss/rest");

    let ice = registry.projectile("Ice_Ball").expect("Ice_Ball");
    let ice = ProjectileAsset::from_value("Ice_Ball", ice).expect("decode ice");
    let mut body = ProjectileBody::launch(ice.clone(), 3, Vec3::new(0.5, 1.0, 0.5), 0.0, 0.0);
    let target = ProjectileTarget {
        id: 99,
        aabb: Aabb::unit().translated(Vec3::new(0.0, 0.0, -2.0)),
    };
    let mut hit = false;
    for _ in 0..40 {
        let step = body.step(&empty, &[target], 0.05);
        if step.events.iter().any(|event| {
            if let ProjectileEvent::Hit { target, .. } = event {
                assert_eq!(*target, 99);
                hit = true;
            }
            hit
        }) {
            break;
        }
    }
    assert!(hit, "the ice ball hits the target box");
    assert_eq!(body.damage_instance().amount, ice.damage as f32);

    let health = EntityStatType {
        id: "Health".to_string(),
        index: 1,
        initial_value: 100.0,
        min: 0.0,
        max: 100.0,
        shared: true,
        regenerating: Vec::new(),
        reset_behavior: StatResetBehavior::MaxValue,
        ignore_invulnerability: false,
        hide_from_tooltip: false,
    };
    let mut gameplay =
        hytale_entities::components::EntityGameplay::new(EntityStatMap::new([health]), 1);
    let causes = DamageCauses::new([DamageCause {
        id: "Projectile".to_string(),
        ..DamageCause::default()
    }]);
    let result = gameplay.apply_damage(&body.damage_instance(), &causes);
    assert_eq!(result.amount, ice.damage as f32);
    assert_eq!(result.remaining_health, 100.0 - ice.damage as f32);
}
