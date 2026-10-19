// Ported from: `.hytale-source/.../worldgen/zone/ZoneDiscoveryConfig.java`
// Ported from: `.hytale-source/.../CoreServer/src/main/java/com/hypixel/hytale/server/core/util/EventTitleUtil.java` (constants)
// Ported from: `.hytale-source/.../CoreServer/src/main/java/com/hypixel/hytale/server/core/util/EventTitleUtil.java` (constants)
//! Java `zone.ZoneDiscoveryConfig` record — zone discovery sound/title
//! settings, plus the [`EventTitleUtil`] constants it defaults from.

/// Java `EventTitleUtil.DEFAULT_ZONE` ("Void").
pub const DEFAULT_ZONE: &str = "Void";

/// Java `EventTitleUtil.DEFAULT_DURATION` (4.0f).
pub const DEFAULT_DURATION: f32 = 4.0;

/// Java `EventTitleUtil.DEFAULT_FADE_DURATION` (1.5f).
pub const DEFAULT_FADE_DURATION: f32 = 1.5;

/// Java `zone.ZoneDiscoveryConfig` record.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneDiscoveryConfig {
    /// Java `boolean display` — whether to display the discovery title and
    /// play the discovery sound.
    pub display: bool,
    /// Java `String zone` — the major zone name this region belongs to.
    pub zone: String,
    /// Java `@Nullable String soundEventId`.
    pub sound_event_id: Option<String>,
    /// Java `@Nullable String icon`.
    pub icon: Option<String>,
    /// Java `boolean major` — whether this zone is a major zone.
    pub major: bool,
    /// Java `float duration`.
    pub duration: f32,
    /// Java `float fadeInDuration`.
    pub fade_in_duration: f32,
    /// Java `float fadeOutDuration`.
    pub fade_out_duration: f32,
}

impl Default for ZoneDiscoveryConfig {
    /// Java `ZoneDiscoveryConfig.DEFAULT`.
    fn default() -> Self {
        Self {
            display: false,
            zone: DEFAULT_ZONE.to_string(),
            sound_event_id: None,
            icon: None,
            major: true,
            duration: DEFAULT_DURATION,
            fade_in_duration: DEFAULT_FADE_DURATION,
            fade_out_duration: DEFAULT_FADE_DURATION,
        }
    }
}

impl ZoneDiscoveryConfig {
    /// Java `ZoneDiscoveryConfig.of(Boolean, String, String, String, Boolean,
    /// Float, Float, Float)` — the nullable-args constructor that fills
    /// defaults from [`EventTitleUtil`] constants where not specified.
    ///
    /// The `Option`-wrapped parameters mirror Java's boxed nullability.
    #[allow(clippy::too_many_arguments)]
    pub fn of(
        display: Option<bool>,
        zone: Option<String>,
        sound_event_id: Option<String>,
        icon: Option<String>,
        major: Option<bool>,
        duration: Option<f32>,
        fade_in_duration: Option<f32>,
        fade_out_duration: Option<f32>,
    ) -> Self {
        Self {
            display: display.unwrap_or(false),
            zone: zone.unwrap_or_else(|| DEFAULT_ZONE.to_string()),
            sound_event_id,
            icon,
            major: major.unwrap_or(true),
            duration: duration.unwrap_or(DEFAULT_DURATION),
            fade_in_duration: fade_in_duration.unwrap_or(DEFAULT_FADE_DURATION),
            fade_out_duration: fade_out_duration.unwrap_or(DEFAULT_FADE_DURATION),
        }
    }
}

// The EventTitleUtil references above point at
// `CoreServer/src/main/java/com/hypixel/hytale/server/core/util/EventTitleUtil.java`.
