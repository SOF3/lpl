use std::collections::{BTreeMap, VecDeque};
use std::time::SystemTime;

use crate::input;

#[derive(Default, Clone)]
pub struct Data {
    pub series_map: BTreeMap<String, Series>,
}

impl Data {
    pub fn push_message(&mut self, message: input::Message) {
        let series = self.series_map.entry(message.label).or_default();
        series.data.push_back(Datum { time: message.time, value: message.value });
    }

    pub fn trim(&mut self, epoch: SystemTime) {
        for series in self.series_map.values_mut() {
            let par_pt = series.data.partition_point(|datum| datum.time < epoch);
            series.data.drain(..par_pt);
        }

        self.series_map.retain(|_, series| !series.data.is_empty());
    }
}

#[derive(Default, Clone)]
pub struct Series {
    pub data: VecDeque<Datum>,
}

#[derive(Clone)]
pub struct Datum {
    pub time:  SystemTime,
    pub value: f64,
}
