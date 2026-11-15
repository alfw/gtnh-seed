#![cfg(feature = "json")]
use rwg2_biomes::forestry::{
    humidity_class, temperature_class, Humidity, Temperature, TemperatureNoise,
};

fn dump21() -> std::path::PathBuf {
    std::env::var("RWG2_DUMP_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../testdata/pack/gtnh-2.9.0/dump")
        })
        .parent()
        .unwrap()
        .join("dump21")
}

fn bits(v: &serde_json::Value) -> f32 {
    f32::from_bits(u32::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap())
}

#[test]
fn forestry_matches_pack() {
    let Ok(s) = std::fs::read_to_string(dump21().join("forestry.json")) else {
        eprintln!("skip");
        return;
    };
    let f: serde_json::Value = serde_json::from_str(&s).unwrap();
    let noise = TemperatureNoise::new();
    // getFloatTemperature samples (plains, 0.8)
    let ts = &f["temperatureSamples"];
    let base = f["biomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == ts["biomeId"])
        .unwrap();
    let t0 = bits(&base["temperature"]);
    let mut bad = 0;
    for s in ts["samples"].as_array().unwrap() {
        let a = s.as_array().unwrap();
        let (x, y, z) = (
            a[0].as_i64().unwrap() as i32,
            a[1].as_i64().unwrap() as i32,
            a[2].as_i64().unwrap() as i32,
        );
        let want = f32::from_bits(u32::from_str_radix(a[3].as_str().unwrap(), 16).unwrap());
        let got = noise.float_temperature(t0, x, y, z);
        if got.to_bits() != want.to_bits() {
            if bad < 5 {
                eprintln!("({x},{y},{z}) got {got} want {want}");
            }
            bad += 1;
        }
    }
    // classes for every biome
    let mut bad_c = 0;
    for b in f["biomes"].as_array().unwrap() {
        let hell = b["hellish"].as_bool().unwrap();
        let dict_nether = b["dictionary"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t == "NETHER");
        assert_eq!(hell, dict_nether, "{b}");
        let t = if hell {
            Temperature::Hellish
        } else {
            temperature_class(bits(&b["temperature"]))
        };
        let h = humidity_class(bits(&b["rainfall"]));
        let tn = format!("{t:?}").to_uppercase();
        let hn = match h {
            Humidity::Arid => "ARID",
            Humidity::Normal => "NORMAL",
            Humidity::Damp => "DAMP",
        };
        if b["forestryTemperature"] != tn.as_str() || b["forestryHumidity"] != hn {
            bad_c += 1;
            eprintln!(
                "{} {} {tn} {hn}",
                b["forestryTemperature"], b["forestryHumidity"]
            );
        }
    }
    eprintln!("temperature samples bad {bad}, class mismatches {bad_c}");
    assert_eq!((bad, bad_c), (0, 0));
}
