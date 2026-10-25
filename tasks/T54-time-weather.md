# T54 — Time and weather

**Status:** done (session 43)
**Package:** `hytale-world`, `hytale-server`
**Depends on:** T43, T48
**Agent fit:** medium

## Goal

Finish the time model and add authoritative per-world weather: load the
`Weather` store, derive each environment's hourly forecast, apply
`WorldConfig.ForcedWeather`, transition the effective weather, and expose the
state to sessions for `UpdateWeather`.

## Deliverables

- `hytale-assets`:
  - load `Server/Weathers` as `Weather` ids with Java's preloaded `Unknown` at
    id 0;
  - retain raw weather documents/fields needed by the state and packet layers.
- `hytale-world::weather`:
  - `WeatherForecast`/`EnvironmentForecast` parsing from
    `Environment.WeatherForecasts` keyed by the 24 in-game hours, including
    `WeatherId`/`Weight` and `WeatherForecastSeed`;
  - deterministic weighted selection seeded by world seed, environment seed
    key and game time;
  - `WeatherState` with `ForcedWeather`, per-environment mapping, hour-change
    detection, current/previous effective weather and transition seconds
    (`0.5` on join, `10` on change);
  - environment lookup at a world position.
- `hytale-world::world_clock`:
  - current hour/day helpers and moon-phase calculation
    (`totalMoonPhases`, midday swap) to finish the T43 time model.
- `hytale-server`:
  - world-thread weather snapshot/command for a player position;
  - session `UpdateWeather` sends on join and when the effective weather changes,
    using the existing Game/stats broadcast loop;
  - `config.json` `ForcedWeather` round-trip.
- Tests:
  - scripted forecast selection/hour change and forced-weather transitions;
  - moon phase/current-hour clock tests;
  - real-pack weather/environment metadata tests;
  - existing time/world/session gates stay green.

## Read first

- Java `builtin/Weather/{WeatherPlugin,resources/WeatherResource,systems/WeatherSystem,components/WeatherTracker}`,
  `asset/type/weather/config/Weather`,
  `asset/type/environment/config/{Environment,WeatherForecast}`,
  `modules/time/WorldTimeResource`
- `crates/hytale-world/src/world_clock.rs`, `world.rs`, `world_config.rs`
- `crates/hytale-server/src/{world_thread,session}.rs`
- Assets: `Server/Weathers/**`, `Server/Environments/**`

## Acceptance

```sh
cargo test -p hytale-world
cargo test -p hytale-server
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- An environment's forecast picks the same weighted weather for the same
  world seed/hour, changes only on the in-game hour, and `ForcedWeather`
  overrides it.
- A weather change produces the Java transition duration; the first send for a
  session uses the join duration.
- Moon phase follows the configured phase count and swaps at midday.
- Existing time/config/save/join gates stay green.

## Deviations to report or split

- Full `UpdateWeathers` weather-document packet byte parity is tracked with
  asset packet goldens; this task wires the store and index/state seam first.
- Weather particles/screen effects/audio are presentation (T57).
- Weather-driven fire uses the T52b fire ticker seam.
- Weather commands and editor preview remain later tooling (T60).

## Result (session 43)

Landed the weather store, 24-hour environment forecast selection,
`ForcedWeather`, effective-weather lookup, current-hour/moon-phase clock
helpers, `WorldThread::weather_at`, and session `UpdateWeather` sends on join
and change. Verified with real-pack forecast tests, clock tests, a forced-weather
world-thread test and the workspace gates.

**Remaining split:** full `UpdateWeathers` asset-document encoder/golden and
config-burst wiring, weather presentation (particles/screen effects/audio),
forced-weather commands/editor preview and a live client-bot transition loop.
