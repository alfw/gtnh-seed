//! Hardcore Ender Expansion End towers: the block layout of `ComponentTower` (32 × 140 × 32 structure space),
//! reimplemented from its behaviour. The layout comes from `Random(((startX / 14)·185226 + (startZ / 14)·24071632895)
//! ^ seed)`; the rotation (`coordBaseMode`) from the `MapGenTower` Random, which `MapGenBase` reseeds per candidate
//! chunk (`(cx·l) ^ (cz·m) ^ seed`, l/m = first two longs of `Random(seed)`), then one `nextInt()` and `nextInt(4)`.
//! Blocks are placed at world (min + 8 + x, 32 + y, min + 8 + z) through the component's rotation, min = start·16.
//! Tile-entity contents (loot, spawner effects) use the world Random and are not modelled.

use crate::java_random::JavaRandom;

pub const SX: i32 = 32;
pub const SY: i32 = 140;
pub const SZ: i32 = 32;
const ROOM_H: i32 = 6;
const ROOMS: i32 = 4;

/// Block categories of a tower (the codes the grid stores).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TB {
    Air = 0,
    EndStone,
    Obsidian,
    ObsidianSpecial,
    ObsidianGlow,
    ObsidianStairs,
    Ladder,
    Glowstone,
    Chest,
    TrappedChest,
    Spawner,
    Head,
    Dispenser,
    Brewing,
    EnchTable,
    Bookshelf,
    FlowerPot,
    Redstone,
    Anvil,
    Furnace,
    Hopper,
    NetherFence,
    Torch,
    StoneBrickStairs,
    OakStairs,
    NetherBrickStairs,
    DoubleSlab,
    StoneSlab,
    IronBars,
    Lamp,
}

impl TB {
    /// Full opaque cube (`Block.isNormalCube`), as far as the redstone check cares.
    fn normal(self) -> bool {
        matches!(
            self,
            TB::EndStone
                | TB::Obsidian
                | TB::ObsidianSpecial
                | TB::ObsidianGlow
                | TB::Bookshelf
                | TB::DoubleSlab
                | TB::Lamp
                | TB::Dispenser
                | TB::Furnace
        )
    }
    /// Solid enough to draw as terrain in the 3D view / count as a block.
    pub fn solid(self) -> bool {
        !matches!(self, TB::Air | TB::Ladder | TB::Redstone | TB::Torch)
    }
}

/// Basic room kinds (`generateBasicRoom` n = 1..9).
pub const ROOM_NAMES: [&str; 10] = [
    "",
    "enchanting table, bookshelves, 2 brewing stands",
    "spawner, head dispensers, redstone",
    "flower pot, 2 spawners, heads",
    "4 dispensers with heads, 8 corner spawners",
    "anvils, heads, 4 spawners, 4 brewing stands",
    "ceiling spawner, iron bars, flower pots",
    "8 spawners, anvils, redstone lamps",
    "maze of pillars, spawners, 1 chest",
    "enchanting table, 2 spawners, bookshelves",
];
/// Top chest rooms (`generateChestRoom` n = 1..3).
pub const CHEST_ROOM_NAMES: [&str; 4] = [
    "",
    "2 chests (7–11 items)",
    "16 small chests + 8 furnaces",
    "1 chest (17–21 items)",
];

pub struct Tower {
    pub cx: i32,
    pub cz: i32,
    /// `coordBaseMode` 0..3
    pub mode: i32,
    /// structure Y of the island surface / ground floor (world Y = 32 + this)
    pub island_y: i32,
    /// basic room kinds of floors 1..3 (1..9)
    pub rooms: [u8; 3],
    /// chest room kind of floor 4 (1..3)
    pub chest_room: u8,
    /// x + SX·(z + SZ·y)
    pub grid: Vec<u8>,
}

impl Tower {
    pub fn at(&self, x: i32, y: i32, z: i32) -> TB {
        if !(0..SX).contains(&x) || !(0..SY).contains(&y) || !(0..SZ).contains(&z) {
            return TB::Air;
        }
        unsafe { std::mem::transmute::<u8, TB>(self.grid[(x + SX * (z + SZ * y)) as usize]) }
    }
    /// World (x, z) of structure (x, z).
    pub fn world_xz(&self, x: i32, z: i32) -> (i32, i32) {
        world_xz(self.mode, self.cx, self.cz, x, z)
    }
    /// World Y of structure y.
    pub fn world_y(y: i32) -> i32 {
        32 + y
    }
    /// World (x, z) of the tower axis.
    pub fn centre(&self) -> (i32, i32) {
        self.world_xz(SX / 2, SZ / 2)
    }
    /// Every non-air block as (world x, y, z, block).
    pub fn blocks(&self) -> Vec<(i32, i32, i32, TB)> {
        let mut v = Vec::new();
        for y in 0..SY {
            for z in 0..SZ {
                for x in 0..SX {
                    let b = self.at(x, y, z);
                    if b != TB::Air {
                        let (wx, wz) = self.world_xz(x, z);
                        v.push((wx, Self::world_y(y), wz, b));
                    }
                }
            }
        }
        v
    }
}

struct Gen {
    g: Vec<u8>,
    r: JavaRandom,
    last: i32,
}

impl Gen {
    fn set(&mut self, x: i32, y: i32, z: i32, b: TB) {
        if (0..SX).contains(&x) && (0..SY).contains(&y) && (0..SZ).contains(&z) {
            self.g[(x + SX * (z + SZ * y)) as usize] = b as u8;
        }
    }
    fn get(&self, x: i32, y: i32, z: i32) -> TB {
        if !(0..SX).contains(&x) || !(0..SY).contains(&y) || !(0..SZ).contains(&z) {
            return TB::Air;
        }
        unsafe { std::mem::transmute::<u8, TB>(self.g[(x + SX * (z + SZ * y)) as usize]) }
    }
    fn int(&mut self, n: i32) -> i32 {
        self.r.next_int_bounded(n)
    }
    fn float(&mut self) -> f32 {
        self.r.next_float()
    }
    fn brewing(&mut self, x: i32, y: i32, z: i32) {
        self.int(100);
        self.set(x, y, z, TB::Brewing);
    }
    fn anvil(&mut self, x: i32, y: i32, z: i32) {
        self.float();
        self.set(x, y, z, TB::Anvil);
    }
    fn furnace(&mut self, x: i32, y: i32, z: i32) {
        self.set(x, y, z, TB::Furnace);
        self.float();
    }

    fn wall_decorations(&mut self, x: i32, y: i32, z: i32) {
        for a in 0..2 {
            for zz in z - 2..=z + 2 {
                self.set(x - 5 + 10 * a, y + 6, zz, TB::ObsidianSpecial);
            }
            for xx in x - 2..=x + 2 {
                self.set(xx, y + 6, z - 5 + 10 * a, TB::ObsidianSpecial);
            }
            for b in 0..2 {
                self.set(x - 4 + 8 * a, y + 6, z - 3 + 6 * b, TB::ObsidianSpecial);
                self.set(x - 3 + 6 * a, y + 6, z - 4 + 8 * b, TB::ObsidianSpecial);
            }
            for yy in y + 1..=y + 5 {
                self.set(x - 5 + 10 * a, yy, z, TB::ObsidianSpecial);
                self.set(x, yy, z - 5 + 10 * a, TB::ObsidianSpecial);
            }
            for b in 0..2 {
                self.set(x - 4 + 8 * a, y + 3, z - 1 + 2 * b, TB::ObsidianGlow);
                self.set(x - 1 + 2 * b, y + 3, z - 4 + 8 * a, TB::ObsidianGlow);
                for c in 0..2 {
                    self.set(x - 5 + 10 * a, y + 1 + 4 * c, z - 1 + 2 * b, TB::Obsidian);
                    self.set(
                        x - 5 + 10 * a,
                        y + 1 + 4 * c,
                        z - 2 + 4 * b,
                        TB::ObsidianStairs,
                    );
                    self.set(
                        x - 5 + 10 * a,
                        y + 2 + 2 * c,
                        z - 1 + 2 * b,
                        TB::ObsidianStairs,
                    );
                    self.set(x - 1 + 2 * b, y + 1 + 4 * c, z - 5 + 10 * a, TB::Obsidian);
                    self.set(
                        x - 2 + 4 * b,
                        y + 1 + 4 * c,
                        z - 5 + 10 * a,
                        TB::ObsidianStairs,
                    );
                    self.set(
                        x - 1 + 2 * b,
                        y + 2 + 2 * c,
                        z - 5 + 10 * a,
                        TB::ObsidianStairs,
                    );
                }
            }
        }
    }

    fn floor(&mut self, x: i32, y: i32, z: i32, room: i32) {
        for a in 0..2 {
            for b in 0..2 {
                for xx in x - 2 + a * 3..=x - 1 + a * 3 {
                    for zz in z - 3 + b * 4..=z - 2 + b * 5 {
                        self.set(xx, y, zz, TB::Obsidian);
                    }
                }
                for xx in x - 2 + a * 3..=x - 1 + a * 3 {
                    self.set(xx, y, z - 1 + b * 2, TB::ObsidianSpecial);
                }
                self.set(x - 3 + a * 6, y, z - 2 + b * 4, TB::ObsidianSpecial);
            }
            self.set(x, y, z - 2 + a * 4, TB::ObsidianSpecial);
            for zz in z - 1..=z + 1 {
                self.set(x - 3 + a * 6, y, zz, TB::Obsidian);
            }
            for xx in x - 2 + a * 3..=x - 1 + a * 3 {
                self.set(xx, y, z, TB::Obsidian);
            }
            for zz in z - 1 + a * 2..=z + a {
                self.set(x, y, zz, TB::Obsidian);
            }
        }
        self.set(x, y, z, TB::ObsidianSpecial);
        if room % 2 == 0 {
            self.set(x, y, z - 3, TB::ObsidianSpecial);
        } else {
            self.set(x, y, z + 3, TB::ObsidianSpecial);
        }
    }

    fn redstone(&mut self, x: i32, y: i32, z: i32, base: i32, spread: i32) {
        let mut a = 0;
        while a < base + self.int(spread) {
            let xx = x + self.int(7) - 3;
            let zz = z + self.int(7) - 3;
            if self.get(xx, y, zz) == TB::Air && self.get(xx, y - 1, zz).normal() {
                self.set(xx, y, zz, TB::Redstone);
            }
            a += 1;
        }
    }

    fn basic_room(&mut self, x: i32, y: i32, z: i32, room: i32) -> i32 {
        let mut n = if room == 0 {
            self.int(8) + 1
        } else {
            self.int(9) + 1
        };
        let even = room % 2 == 0;
        if n == self.last {
            n = self.int(9) + 1;
        }
        self.last = n;
        let ez = if even { 3 } else { -3 };
        match n {
            1 => {
                self.set(x, y, z, TB::EnchTable);
                if self.int(3) != 0 {
                    self.int(5);
                }
                for a in 0..2 {
                    self.set(x - 3 + a * 6, y + 4, z, TB::FlowerPot);
                    self.set(x - 2 + a * 4, y, z + ez, TB::ObsidianSpecial);
                    self.brewing(x - 2 + a * 4, y + 1, z + ez);
                    for b in 0..2 {
                        for yy in y..=y + 3 {
                            self.set(x - 3 + a * 6, yy, z - 1 + b * 2, TB::Bookshelf);
                        }
                        self.set(x - 3 + a * 6, y + 4, z - 2 + b * 4, TB::Head);
                        for c in 0..2 {
                            self.set(x - 3 + a * 6, y + c * 3, z + 2 - b * 4, TB::OakStairs);
                        }
                    }
                    for b in 0..4 {
                        self.set(x - 3 + a * 6, y + b, z, TB::OakStairs);
                    }
                }
            }
            2 => {
                self.set(x, y, z, TB::Spawner);
                for a in 0..2 {
                    self.set(x - 3 + a * 6, y, z, TB::Dispenser);
                    self.set(x - 3 + a * 6, y + 1, z, TB::Head);
                }
                self.redstone(x, y, z, 6, 10);
            }
            3 => {
                self.set(x, y, z, TB::Dispenser);
                self.set(x, y + 1, z, TB::FlowerPot);
                self.int(3);
                for a in 0..2 {
                    self.set(x - 3 + a * 6, y, z, TB::Spawner);
                    for b in 0..2 {
                        self.set(x - 3 + a * 6, y + 2, z - 2 + b * 4, TB::ObsidianStairs);
                        self.set(x - 3 + a * 6, y + 3, z - 2 + b * 4, TB::Head);
                    }
                }
            }
            4 => {
                for a in 0..2 {
                    self.set(x - 3 + a * 6, y, z, TB::Dispenser);
                    self.set(x - 3 + a * 6, y + 1, z, TB::Head);
                    for b in 0..2 {
                        self.set(x - 3 + a * 6, y + 2, z - 2 + b * 4, TB::Spawner);
                        self.set(x - 2 + a * 4, y + 2, z - 3 + b * 6, TB::Spawner);
                        for c in 0..2 {
                            for d in 0..2 {
                                self.set(
                                    x - 3 + a * 6,
                                    y + c + 3 * d,
                                    z - 2 + b * 4,
                                    TB::StoneBrickStairs,
                                );
                                self.set(
                                    x - 2 + a * 4,
                                    y + c + 3 * d,
                                    z - 3 + b * 6,
                                    TB::StoneBrickStairs,
                                );
                            }
                        }
                    }
                }
            }
            5 => {
                for a in 0..2 {
                    self.set(x - 3 + a * 6, y, z, TB::DoubleSlab);
                    self.set(x - 3 + a * 6, y + 1, z, TB::Head);
                    for b in 0..2 {
                        self.anvil(x - 3 + a * 6, y, z - 1 + b * 2);
                        self.set(x - 3 + a * 6, y, z - 2 + b * 4, TB::DoubleSlab);
                        self.set(x - 3 + a * 6, y + 1, z - 2 + b * 4, TB::StoneBrickStairs);
                        for yy in y + 2..=y + 3 {
                            self.set(x - 3 + a * 6, yy, z - 2 + b * 4, TB::NetherFence);
                        }
                        self.set(x - 3 + a * 6, y + 4, z - 2 + b * 4, TB::Spawner);
                        self.set(x - 1 + a * 2, y, z - 3 + b * 6, TB::StoneBrickStairs);
                        self.set(x - 2 + a * 4, y, z - 3 + b * 6, TB::Bookshelf);
                        self.brewing(x - 2 + a * 4, y + 1, z - 3 + b * 6);
                    }
                }
            }
            6 => {
                self.set(x, y + 4, z, TB::Spawner);
                for a in 0..2 {
                    for b in 0..2 {
                        self.set(x - 3 + a * 6, y + b, z, TB::NetherBrickStairs);
                        self.set(x - 3 + a * 6, y, z - 2 + b * 4, TB::Dispenser);
                        self.int(5);
                        self.set(x - 3 + a * 6, y + 1, z - 2 + b * 4, TB::FlowerPot);
                    }
                    self.set(x - 3 + a * 6, y + 4, z, TB::ObsidianSpecial);
                    for yy in y + 2..=y + 3 {
                        self.set(x - 3 + a * 6, yy, z, TB::IronBars);
                    }
                    for yy in y + 3..=y + 4 {
                        self.set(x - 2 + a * 4, yy, z, TB::IronBars);
                    }
                    self.set(x - 1 + a * 2, y + 5, z, TB::IronBars);
                }
                self.redstone(x, y, z, 5, 8);
            }
            7 => {
                let zo = if even { -3 } else { 3 };
                self.set(x, y + 4, z - zo, TB::StoneBrickStairs);
                for a in 0..2 {
                    for yy in y + 1..=y + 3 {
                        self.set(x - 3 + a * 6, yy, z, TB::Bookshelf);
                    }
                    for b in 0..2 {
                        self.set(x - 3 + a * 6, y + 2, z - 1 + b * 2, TB::Bookshelf);
                        self.set(x - 1 + b * 2, y + 2, z - 3 + a * 6, TB::Bookshelf);
                        self.set(x - 3 + a * 6, y, z - 2 + b * 4, TB::Spawner);
                        self.set(x - 2 + a * 4, y, z - 3 + b * 6, TB::Spawner);
                        self.anvil(x - 2 + a * 4, y, z - 2 + b * 4);
                        self.set(x - 2 + a * 4, y + 1, z - 2 + b * 4, TB::Lamp);
                        self.set(x - 2 + a * 4, y + 2, z - 2 + b * 4, TB::StoneSlab);
                        self.set(x - 3 + a * 6, y + b * 4, z, TB::StoneBrickStairs);
                        for c in 0..2 {
                            for d in 0..2 {
                                self.set(
                                    x - 3 + a * 6,
                                    y + c + 3 * d,
                                    z - 1 + b * 2,
                                    TB::StoneBrickStairs,
                                );
                                self.set(
                                    x - 1 + a * 2,
                                    y + c + 3 * d,
                                    z - 3 + b * 6,
                                    TB::StoneBrickStairs,
                                );
                            }
                        }
                    }
                }
            }
            8 => {
                const P: [(i32, i32); 9] = [
                    (-2, -1),
                    (0, -2),
                    (1, -2),
                    (1, -3),
                    (-3, 2),
                    (-1, 2),
                    (-1, 3),
                    (0, 1),
                    (3, 1),
                ];
                for (px, pz) in P {
                    for yy in y..=y + 4 {
                        self.set(x + px, yy, z + pz, TB::ObsidianSpecial);
                    }
                    if self.float() < 0.25 {
                        let sy = y + self.int(5);
                        self.set(x + px, sy, z + pz, TB::Spawner);
                    }
                }
                for yy in y..=y + 1 {
                    self.set(x + 2, yy, z - 1, TB::ObsidianSpecial);
                }
                self.set(x + 2, y + 2, z - 1, TB::Spawner);
                self.set(x + 2, y + 3, z - 1, TB::ObsidianStairs);
                let (mut attempt, mut placed) = (0, 0);
                while attempt < 8 && placed < self.int(4) {
                    let xx = x + self.int(7) - 3;
                    let zz = z + self.int(7) - 3;
                    if self.get(xx, y, zz) == TB::Air {
                        self.set(xx, y, zz, TB::Spawner);
                        placed += 1;
                    }
                    attempt += 1;
                }
                let trapped = self.r.next_boolean();
                self.set(
                    x + 2,
                    y,
                    z - 3,
                    if trapped { TB::TrappedChest } else { TB::Chest },
                );
            }
            _ => {
                self.set(x, y, z, TB::EnchTable);
                for yy in y + 3 + i32::from(room == 0)..=y + 4 {
                    self.set(x, yy, z + ez, TB::Obsidian);
                }
                for a in 0..2 {
                    self.set(x - 3 + a * 6, y + 2, z, TB::Spawner);
                    self.set(x, y + 4, z - 2 + a * 4, TB::ObsidianStairs);
                    for b in 0..2 {
                        for yy in y + b * 3..=y + 1 + b * 3 {
                            self.set(x - 3 + a * 6, yy, z, TB::Obsidian);
                        }
                        self.set(x - 2 + 4 * a, y + 4 * b, z, TB::ObsidianStairs);
                        for yy in y..=y + 4 {
                            self.set(x - 3 + a * 6, yy, z - 2 + b * 4, TB::Bookshelf);
                        }
                        for yy in y..=y + 4 {
                            self.set(x - 2 + a * 4, yy, z - 3 + b * 6, TB::Bookshelf);
                        }
                        self.set(x - 3 + a * 6, y, z - 1 + b * 2, TB::OakStairs);
                        self.set(x - 1 + a * 2, y, z - 3 + b * 6, TB::OakStairs);
                        for c in 0..2 {
                            if self.float() < 0.42 {
                                let (xx, zz) = if c == 0 {
                                    (x - 3 + a * 6, z - 1 + b * 2)
                                } else {
                                    (x - 1 + a * 2, z - 3 + b * 6)
                                };
                                if self.r.next_boolean() {
                                    if self.int(10) != 0 {
                                        self.int(4);
                                    }
                                    self.set(xx, y + 1, zz, TB::FlowerPot);
                                } else {
                                    self.brewing(xx, y + 1, zz);
                                }
                            }
                        }
                    }
                }
            }
        }
        n
    }

    fn chest_room(&mut self, x: i32, y: i32, z: i32, room: i32) -> i32 {
        let n = self.int(3) + 1;
        let even = room % 2 == 0;
        match n {
            1 => {
                for a in 0..2 {
                    self.set(x - 3 + a * 6, y, z, TB::Chest);
                    for b in 0..2 {
                        self.set(x - 3 + a * 6, y, z - 2 + b * 4, TB::Spawner);
                        self.set(x - 2 + a * 4, y, z - 3 + b * 6, TB::Spawner);
                        self.set(x - 2 + a * 4, y, z - 2 + b * 4, TB::ObsidianSpecial);
                        self.set(x - 2 + a * 4, y + 1, z - 2 + b * 4, TB::Head);
                        for yy in y + 2..=y + 3 {
                            self.set(x - 3 + a * 6, yy, z - 2 + b * 4, TB::NetherFence);
                        }
                        self.set(x - 3 + a * 6, y + 4, z - 2 + b * 4, TB::Hopper);
                        for yy in y + 2..=y + 3 {
                            self.set(x - 2 + a * 4, yy, z - 3 + b * 6, TB::NetherFence);
                        }
                        self.set(x - 2 + a * 4, y + 4, z - 3 + b * 6, TB::Hopper);
                        self.set(x - 3 + a * 6, y, z - 1 + b * 2, TB::ObsidianStairs);
                        self.set(x - 3 + a * 6, y + 1, z - 2 + b * 4, TB::ObsidianStairs);
                        self.set(x - 1 + b * 2, y, z - 3 + a * 6, TB::ObsidianStairs);
                        self.set(x - 2 + b * 4, y + 1, z - 3 + a * 6, TB::ObsidianStairs);
                    }
                }
            }
            2 => {
                for a in 0..2 {
                    self.set(x - 3 + a * 6, y, z, TB::Spawner);
                    for yy in y + 1..=y + 3 {
                        self.set(x - 3 + a * 6, yy, z, TB::NetherFence);
                    }
                    self.set(x - 3 + a * 6, y + 4, z, TB::Hopper);
                    for b in 0..2 {
                        for c in 0..2 {
                            self.int(5);
                            self.set(
                                x - 3 + a * 6,
                                y,
                                z + (c + 1) * (-1 + b * 2),
                                TB::TrappedChest,
                            );
                            self.int(5);
                            self.set(
                                x + (c + 1) * (-1 + b * 2),
                                y,
                                z - 3 + a * 6,
                                TB::TrappedChest,
                            );
                            self.set(
                                x - 3 + a * 6,
                                y + 2 + c * 2,
                                z - 1 + b * 2,
                                TB::StoneBrickStairs,
                            );
                            self.set(
                                x - 3 + a * 6,
                                y + 2 + c * 2,
                                z - 2 + b * 4,
                                TB::StoneBrickStairs,
                            );
                            self.set(
                                x - 1 + a * 2,
                                y + 2 + c * 2,
                                z - 3 + b * 6,
                                TB::StoneBrickStairs,
                            );
                            self.set(
                                x - 2 + a * 4,
                                y + 2 + c * 2,
                                z - 3 + b * 6,
                                TB::StoneBrickStairs,
                            );
                        }
                        self.set(x - 2 + a * 4, y + 4, z - 2 + b * 4, TB::StoneBrickStairs);
                        self.furnace(x - 3 + a * 6, y + 3, z - 2 + b * 4);
                        self.furnace(x - 2 + a * 4, y + 3, z - 3 + b * 6);
                    }
                }
                let zo = if even { 3 } else { -3 };
                for yy in y + 2..=y + 3 {
                    self.set(x, yy, z + zo, TB::NetherFence);
                }
                self.set(x, y + 4, z + zo, TB::Hopper);
            }
            _ => {
                self.set(x, y, z, TB::Chest);
                self.set(x, y + 2, z, TB::ObsidianGlow);
                self.set(x, y + 3, z, TB::Spawner);
                for a in 0..2 {
                    self.set(x - 1 + a * 2, y + 3, z, TB::Obsidian);
                    self.set(x, y + 3, z - 1 + a * 2, TB::Obsidian);
                    self.set(x - 2 + a * 4, y + 4, z, TB::Obsidian);
                    self.set(x, y + 4, z - 2 + a * 4, TB::Obsidian);
                    for b in 0..2 {
                        self.set(x - 1 + a * 2, y + 4, z - 1 + b * 2, TB::Obsidian);
                        self.set(x - 3 + a * 6, y, z - 2 + b * 4, TB::NetherFence);
                        self.set(x - 3 + a * 6, y + 1, z - 2 + b * 4, TB::Torch);
                        self.set(x - 2 + a * 4, y, z - 3 + b * 6, TB::NetherFence);
                        self.set(x - 2 + a * 4, y + 1, z - 3 + b * 6, TB::Torch);
                    }
                }
            }
        }
        n
    }
}

/// World (x, z) of structure (x, z) of a tower started at chunk (cx, cz) with rotation `mode`.
pub fn world_xz(mode: i32, cx: i32, cz: i32, x: i32, z: i32) -> (i32, i32) {
    let (mx, mz) = (cx * 16, cz * 16);
    let (xx, zz) = (8 + x, 8 + z);
    match mode {
        1 => (mx + 31 - zz, mz + xx),
        2 => (mx + xx, mz + 31 - zz),
        3 => (mx + zz, mz + xx),
        _ => (mx + xx, mz + zz),
    }
}

/// World (x, z) of the axis of the tower started at chunk (cx, cz); `terrain_seed` = seed + HEE resets.
pub fn tower_centre(terrain_seed: i64, cx: i32, cz: i32) -> (i32, i32) {
    world_xz(tower_mode(terrain_seed, cx, cz), cx, cz, SX / 2, SZ / 2)
}

/// `coordBaseMode` of the tower started at chunk (cx, cz): `MapGenBase.rand` (Hodgepodge `fastRandom` = StdLCG,
/// `setSeed` stores the seed raw → `JavaRandom::new(s ^ 0x5DEECE66D)` for both seedings), then
/// `MapGenStructure`'s `rand.nextInt()` and `ComponentScatteredFeatureCustom`'s `nextInt(4)`. ✅ rwg2dump 2.14
/// (a fresh tower: real `coordBaseMode` 1; the unscrambled-vanilla seeding gave 3).
pub fn tower_mode(seed: i64, cx: i32, cz: i32) -> i32 {
    const M: i64 = 0x5DEECE66D;
    let mut r = JavaRandom::new(seed ^ M);
    let l = r.next_long();
    let m = r.next_long();
    let mut r =
        JavaRandom::new((cx as i64).wrapping_mul(l) ^ (cz as i64).wrapping_mul(m) ^ seed ^ M);
    r.next_int();
    r.next_int_bounded(4)
}

/// The tower started at chunk (cx, cz) (call only for real starts, see `end::tower_at`). `seed` = world seed (the
/// layout uses `getWorldInfo().getSeed()`), `resets` = HEE End resets (the rotation uses `world.getSeed()`).
pub fn tower(seed: i64, resets: i32, cx: i32, cz: i32) -> Tower {
    let (sx, sz) = (cx * 16, cz * 16);
    let r = JavaRandom::new(
        ((sx / 14) as i64)
            .wrapping_mul(185_226)
            .wrapping_add(((sz / 14) as i64).wrapping_mul(24_071_632_895))
            ^ seed,
    );
    let mut g = Gen {
        g: vec![0; (SX * SY * SZ) as usize],
        r,
        last: -1,
    };
    let (x, z) = (SX / 2, SZ / 2);
    let iy = 32 + g.int(40);
    // island
    let rad = g.float() * 3.5 + 9.0;
    let ceil_rad = rad.ceil() as i32;
    for a in x - ceil_rad..=x + ceil_rad {
        for b in x - ceil_rad..=x + ceil_rad {
            let d = (((a - x) * (a - x) + (b - x) * (b - x)) as f64).sqrt();
            let f1 = g.float();
            let f2 = g.float();
            if d > (rad - f1 * 1.1 + f2 * 0.5) as f64 {
                continue;
            }
            let df = ceil_rad as f64 * 1.15 - d;
            let mut sy = 0;
            loop {
                let lim = df * 0.45 * (df * 0.5).sqrt() + g.r.next_double() * 1.5 * df;
                if (sy as f64) > lim {
                    break;
                }
                g.set(a, iy - sy, b, TB::EndStone);
                sy += 1;
            }
        }
    }
    // walls
    let top = iy + ROOM_H * ROOMS;
    for a in 0..2 {
        for b in 0..5 {
            for y in iy..=top {
                g.set(x - 4 + 8 * a, y, z - 2 + b, TB::ObsidianSpecial);
                g.set(x - 2 + b, y, z - 4 + 8 * a, TB::ObsidianSpecial);
            }
        }
        for b in 0..2 {
            for y in iy..=top {
                g.set(x - 3 + 6 * a, y, z - 3 + 6 * b, TB::ObsidianSpecial);
            }
        }
    }
    // rooms
    let mut rooms = [0u8; 3];
    let mut chest = 0u8;
    for a in 0..ROOMS {
        let yy = iy + ROOM_H * a;
        for py in yy + 1..=yy + 6 {
            g.set(x, py, z - 3 + 6 * (a % 2), TB::Ladder);
        }
        g.wall_decorations(x, yy, z);
        g.floor(x, yy, z, a);
        if a < ROOMS - 1 {
            rooms[a as usize] = g.basic_room(x, yy + 1, z, a) as u8;
        } else {
            chest = g.chest_room(x, yy + 1, z, a) as u8;
        }
    }
    // entrance
    for yy in iy + 1..=iy + 5 {
        for xx in x - 1..=x + 1 {
            g.set(xx, yy, z + 4, TB::Air);
        }
        for xx in x - 2..=x + 2 {
            g.set(xx, yy, z + 5, TB::Air);
        }
    }
    for xx in x - 1..=x + 1 {
        for zz in z + 3..=z + 5 {
            g.set(xx, iy, zz, TB::Obsidian);
        }
        g.set(xx, iy + 5, z + 4, TB::ObsidianSpecial);
    }
    for a in 0..2 {
        for yy in iy..=iy + 5 {
            g.set(x - 2 + 4 * a, yy, z + 5, TB::ObsidianSpecial);
        }
        g.set(x - 1 + 2 * a, iy + 5, z + 5, TB::ObsidianStairs);
    }
    // top
    for xx in x - 3..=x + 3 {
        for zz in z - 2..=z + 2 {
            g.set(xx, top, zz, TB::Obsidian);
        }
    }
    for a in 0..2 {
        for xx in x - 2..=x + 2 {
            g.set(xx, top, z - 3 + 6 * a, TB::Obsidian);
        }
        for (d, y0, y1) in [(5, -2, 5), (6, -1, 8), (7, 0, 10), (8, 3, 12)] {
            for yy in top + y0..=top + y1 {
                g.set(x - d + 2 * d * a, yy, z, TB::Obsidian);
            }
        }
        for b in 0..2 {
            for (d, y0, y1) in [(5, -1, 0), (6, 0, 4), (7, 4, 8)] {
                for yy in top + y0..=top + y1 {
                    g.set(x - d + 2 * d * a, yy, z - 1 + 2 * b, TB::Obsidian);
                }
            }
        }
        g.set(x - 8 + 16 * a, top + 13, z, TB::Glowstone);
        for (d, y0, y1) in [(5, -2, 5), (6, -1, 8), (7, 0, 10), (8, 3, 12)] {
            for yy in top + y0..=top + y1 {
                g.set(x, yy, z - d + 2 * d * a, TB::Obsidian);
            }
        }
        for b in 0..2 {
            for (d, y0, y1) in [(5, -1, 0), (6, 0, 4), (7, 4, 8)] {
                for yy in top + y0..=top + y1 {
                    g.set(x - 1 + 2 * b, yy, z - d + 2 * d * a, TB::Obsidian);
                }
            }
        }
        g.set(x, top + 13, z - 8 + 16 * a, TB::Glowstone);
    }
    let zo = if ROOMS % 2 == 1 { 2 } else { -2 };
    g.set(x, top, z + if ROOMS % 2 == 1 { -3 } else { 3 }, TB::Ladder);
    g.set(x, top + 1, z + zo, TB::ObsidianGlow);
    Tower {
        cx,
        cz,
        mode: tower_mode(seed.wrapping_add(resets as i64), cx, cz),
        island_y: iy,
        rooms,
        chest_room: chest,
        grid: g.g,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tower_shape_sane() {
        let t = tower(12345, 0, 40, -30);
        assert!((32..72).contains(&t.island_y));
        assert!(t.rooms.iter().all(|&r| (1..=9).contains(&r)) && (1..=3).contains(&t.chest_room));
        let n = t.blocks().len();
        assert!(n > 1500 && n < 20000, "{n}");
        // walls: 4-wide ring at the ground floor
        assert_eq!(t.at(16 - 4, t.island_y + 2, 16), TB::ObsidianSpecial);
    }
}
