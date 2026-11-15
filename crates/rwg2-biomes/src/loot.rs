//! Vanilla dungeon chest loot (Forge 10.13.4 `ChestGenHooks("dungeonChest")`), draw-exact.
//!
//! `WorldGenDungeons` fills a chest with
//! `generateChestContents(rand, ChestGenHooks.getItems(DUNGEON_CHEST, rand), chest, getCount(DUNGEON_CHEST, rand))`
//! off the chunk's shared populate Random, so the number of draws decides every later blob in the chunk.
//! - `getItems`: every entry is passed through `Item.getChestGenBase`; only the enchanted book overrides it and
//!   rolls `EnchantmentHelper.addRandomEnchantment(rand, book, 30)` (book enchantability 1): 2× `nextInt(1)`,
//!   2× `nextFloat`, one weighted pick over the enchantment map, then `while nextInt(50) <= j1 { pick; j1 >>= 1 }`.
//!   The enchantment map itself is not modelled: each pick is one `nextInt` (the map never empties in practice).
//! - `getCount`: `min + nextInt(max - min)` only when `min < max` (Forge: max exclusive; rwg2dump 2.17 probes).
//! - per pick: `nextInt(totalWeight)`, `min + nextInt(max - min + 1)`, then one `nextInt(27)` slot per stack
//!   (`count > maxStack` splits into `count` stacks of one).
//!
//! Validated against 400 in-game probes (`dungeonloot.json`, rwg2dump 2.11.0): draw count, items and slots 400/400.

use crate::java_random::JavaRandom;
use std::sync::OnceLock;

const LOOT_JSON: &str = include_str!("../../../data/gtnh-2.9.0/dungeonloot.json");
/// other `ChestGenHooks` categories (rwg2dump 2.17 `chestloot.json` via `tools/chestloot.py`; `{}` until then)
const CHEST_JSON: &str = include_str!("../../../data/gtnh-2.9.0/chestloot.json");

#[derive(Clone, Debug)]
pub struct LootItem {
    pub name: String,
    pub min: i32,
    pub max: i32,
    pub weight: i32,
    pub max_stack: i32,
    pub book: bool,
}

#[derive(Clone, Debug)]
pub struct LootTable {
    pub items: Vec<LootItem>,
    pub total: i32,
    pub count: (i32, i32),
}

/// One filled slot: (slot 0..27, item index, stack size). A later stack in the same slot overwrites.
pub type Slot = (u8, u16, u8);

#[derive(Clone, Debug, Default)]
pub struct Chest {
    /// final contents, by slot
    pub slots: Vec<Slot>,
    /// modified level `i1` of the enchanted book's roll (enchantment itself not modelled)
    pub book_level: i32,
}

impl LootTable {
    /// Parses the compact `{"count":[min,max],"items":[["name",min,max,weight,maxStack],...]}` form (no serde: the
    /// core crate keeps JSON optional).
    pub fn from_json(s: &str) -> Option<LootTable> {
        let nums =
            |t: &str| -> Option<Vec<i32>> { t.split(',').map(|x| x.trim().parse().ok()).collect() };
        let c0 = s.find("\"count\":[")? + 9;
        let c = nums(&s[c0..c0 + s[c0..].find(']')?])?;
        let count = (*c.first()?, *c.get(1)?);
        let mut rest = &s[s.find("\"items\":[")? + 9..];
        let mut items = Vec::new();
        while let Some(o) = rest.find("[\"") {
            let e = o + rest[o..].find(']')?;
            let body = &rest[o + 2..e];
            let q = body.find('"')?;
            let name = body[..q].to_string();
            let n = nums(body[q + 1..].trim_start_matches(','))?;
            items.push(LootItem {
                book: name.starts_with("item.enchantedBook"),
                name,
                min: *n.first()?,
                max: *n.get(1)?,
                weight: *n.get(2)?,
                max_stack: *n.get(3)?,
            });
            rest = &rest[e + 1..];
        }
        let total = items.iter().map(|i| i.weight).sum();
        (!items.is_empty()).then_some(LootTable {
            items,
            total,
            count,
        })
    }

    /// The embedded GTNH dungeon chest table.
    pub fn dungeon() -> &'static LootTable {
        static T: OnceLock<LootTable> = OnceLock::new();
        T.get_or_init(|| LootTable::from_json(LOOT_JSON).expect("dungeonloot.json"))
    }

    /// The GTNH `ChestGenHooks("mineshaftCorridor")` table, when `chestloot.json` holds it (rwg2dump 2.17).
    pub fn mineshaft() -> Option<&'static LootTable> {
        static T: OnceLock<Option<LootTable>> = OnceLock::new();
        T.get_or_init(|| LootTable::category(CHEST_JSON, "mineshaftCorridor"))
            .as_ref()
    }

    /// One category of the `{"<category>":{"count":[..],"items":[..]},...}` form.
    pub fn category(s: &str, cat: &str) -> Option<LootTable> {
        let k = s.find(&format!("\"{cat}\":{{"))?;
        let body = &s[k..];
        let e = body.find('}')?;
        LootTable::from_json(&body[..e])
    }

    /// `getItems` + `getCount` + `generateChestContents` on `r`.
    pub fn roll(&self, r: &mut JavaRandom) -> Chest {
        let (book_level, n) = self.prepare(r);
        self.fill(r, n, book_level)
    }

    /// `ChestGenHooks.getItems` + `getCount` (the arguments, evaluated before the chest is placed): (book level, count).
    pub fn prepare(&self, r: &mut JavaRandom) -> (i32, i32) {
        let mut book_level = 0;
        for it in &self.items {
            if it.book {
                book_level = book_roll(r);
            }
        }
        let n = if self.count.0 < self.count.1 {
            self.count.0 + r.next_int_bounded(self.count.1 - self.count.0)
        } else {
            self.count.0
        };
        (book_level, n)
    }

    /// `WeightedRandomChestContent.generateChestContents` of `n` picks into a 27-slot inventory.
    pub fn fill(&self, r: &mut JavaRandom, n: i32, book_level: i32) -> Chest {
        let mut slots = [None::<(u16, u8)>; 27];
        for _ in 0..n {
            let mut x = r.next_int_bounded(self.total);
            let mut k = 0;
            while k + 1 < self.items.len() && x >= self.items[k].weight {
                x -= self.items[k].weight;
                k += 1;
            }
            let it = &self.items[k];
            let c = it.min + r.next_int_bounded(it.max - it.min + 1);
            if c > it.max_stack {
                for _ in 0..c {
                    slots[r.next_int_bounded(27) as usize] = Some((k as u16, 1));
                }
            } else {
                slots[r.next_int_bounded(27) as usize] = Some((k as u16, c as u8));
            }
        }
        Chest {
            slots: slots
                .iter()
                .enumerate()
                .filter_map(|(s, v)| v.map(|(k, c)| (s as u8, k, c)))
                .collect(),
            book_level,
        }
    }
}

/// `ItemEnchantedBook.func_92112_a` → `addRandomEnchantment(rand, book, 30)`; returns the modified level.
pub fn book_roll(r: &mut JavaRandom) -> i32 {
    // enchantability 1: k /= 2 → 0; k = 1 + nextInt(1) + nextInt(1)
    let k = 1 + r.next_int_bounded(1) + r.next_int_bounded(1);
    let l = k + 30;
    let f = (r.next_float() + r.next_float() - 1.0f32) * 0.15f32;
    let i1 = ((l as f32 * (1.0f32 + f) + 0.5f32) as i32).max(1);
    // weighted pick over the enchantment map (its total is unknown here; any non-power-of-two bound is one draw
    // barring a ~1e-6 rejection)
    r.next_int_bounded(1000);
    let mut j1 = i1;
    while r.next_int_bounded(50) <= j1 {
        r.next_int_bounded(1000);
        j1 >>= 1;
    }
    i1
}
