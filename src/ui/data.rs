use std::collections::{BTreeMap, VecDeque};
use std::time::SystemTime;

use crate::input;

#[derive(Default)]
pub struct Cache {
    pub data:        Freezable,
    pub disp_config: BTreeMap<String, DisplayConfig>,
    color_pool:      ColorPool,
}

pub struct DisplayConfig {
    pub visible: bool,
    pub color:   [u8; 3],
}

impl Cache {
    pub fn push_message(&mut self, message: input::Message) {
        self.disp_config
            .entry(message.label.clone())
            .or_insert_with(|| DisplayConfig { visible: true, color: self.color_pool.next() });

        let series =
            self.data.map.entry(message.label).or_insert_with(|| Series { data: VecDeque::new() });
        series.data.push_back(Datum { time: message.time, value: message.value });
    }

    pub fn trim(&mut self, epoch: SystemTime) {
        for series in self.data.map.values_mut() {
            let par_pt = series.data.partition_point(|datum| datum.time < epoch);
            series.data.drain(..par_pt);
        }

        self.data.map.retain(|_, series| !series.data.is_empty());
    }
}

#[derive(Default, Clone)]
pub struct Freezable {
    pub map: BTreeMap<String, Series>,
}

#[derive(Clone)]
pub struct Series {
    pub data: VecDeque<Datum>,
}

#[derive(Clone)]
pub struct Datum {
    pub time:  SystemTime,
    pub value: f64,
}

const DEFAULT_COLOR_MAP: &[[u8; 3]] = &[
    // Source: Set1 from matplotlib
    [228, 26, 28],
    [55, 126, 184],
    [77, 175, 74],
    [152, 78, 163],
    [255, 127, 0],
    [255, 255, 51],
    [166, 86, 40],
    [247, 129, 191],
    [153, 153, 153],
];

#[derive(Default)]
struct ColorPool {
    next_color: usize,
}

impl ColorPool {
    fn next(&mut self) -> [u8; 3] {
        let offset = self.next_color;
        self.next_color += 1;
        self.next_color %= DEFAULT_COLOR_MAP.len();
        DEFAULT_COLOR_MAP[offset]
    }
}
