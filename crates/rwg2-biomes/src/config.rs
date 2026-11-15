//! `rwg2.config.ConfigRWG` worldgen-relevant values. Java stores these as `float`; they are widened to `double`
//! at exactly the points the Java code does (e.g. `0.15f` compares as 0.15000000596…), so keep them `f32` here.

#[derive(Clone, Debug, PartialEq)]
pub struct RwgConfig {
    pub minimum_continent_width: f32,
    pub maximum_continent_width: f32,
    pub average_ocean_width: f32,
    pub minimum_ocean_width: f32,
    pub maximum_ocean_fraction: f32,
    pub minimum_island_width: f32,
    pub maximum_island_width: f32,
    pub island_placement_chance: f32,
    pub large_island_volcano_chance: f32,
    pub average_landmarks_per_type_and_continent: f32,
    pub biome_size_multiplier: f32,
    pub landmass_offset_x: i32,
    pub landmass_offset_z: i32,
    pub biome_offset_x: i32,
    pub biome_offset_z: i32,
}

impl Default for RwgConfig {
    /// RWG2 2.0.6 defaults (== GTNH 2.9.0 shipped/generated `rwg2.cfg`).
    fn default() -> Self {
        Self {
            minimum_continent_width: 2700.0,
            maximum_continent_width: 2700.0,
            average_ocean_width: 100.0,
            minimum_ocean_width: 50.0,
            maximum_ocean_fraction: 0.4,
            minimum_island_width: 300.0,
            maximum_island_width: 600.0,
            island_placement_chance: 0.30,
            large_island_volcano_chance: 0.15,
            average_landmarks_per_type_and_continent: 0.25,
            biome_size_multiplier: 1.0,
            landmass_offset_x: 0,
            landmass_offset_z: 0,
            biome_offset_x: 0,
            biome_offset_z: 0,
        }
    }
}
