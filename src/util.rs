use anyhow::Result;
use futures::future::pending;
use futures::{Stream, StreamExt as _};
use num_traits::{SaturatingAdd, SaturatingSub};
use ratatui::layout;

pub struct Finally<T, F: FnOnce(T) -> Result<()>>(pub Option<(T, F)>);
impl<T, F: FnOnce(T) -> Result<()>> Drop for Finally<T, F> {
    fn drop(&mut self) {
        let (data, closure) = self.0.take().unwrap();
        if let Err(err) = closure(data) {
            eprintln!("Error: {err}");
        }
    }
}

pub async fn some_or_pending<T>(option: &mut Option<impl Stream<Item = T> + Unpin>) -> T {
    if let Some(stream) = option {
        let item = stream.next().await;
        if let Some(item) = item {
            item
        } else {
            *option = None;
            pending().await
        }
    } else {
        pending().await
    }
}

#[must_use]
pub fn disp_float(value: f64, digits: u32) -> String {
    if value == 0.0 {
        return String::from("0");
    }

    let log = value.abs().log10().floor() as i32;

    let sig_base = 10f64.powi(log - (digits as i32));
    let rounded = (value / sig_base).round();

    if log.abs() >= digits as i32 {
        format!("{:.*}e{log}", digits as usize, rounded * 10f64.powi(-(digits as i32)))
    } else {
        format!("{:.*}", digits as usize, rounded * sig_base)
    }
}

#[must_use]
pub fn center_subrect(rect: layout::Rect, ratio: (u16, u16)) -> layout::Rect {
    let center_x = (rect.left() + rect.right()) / 2;
    let center_y = (rect.top() + rect.bottom()) / 2;
    let new_width = rect.width * ratio.0 / ratio.1;
    let new_height = rect.height * ratio.0 / ratio.1;

    let new_left = center_x - new_width / 2;
    let new_top = center_y - new_height / 2;

    layout::Rect { x: new_left, y: new_top, width: new_width, height: new_height }
}

#[must_use]
pub fn rect_resize(
    mut rect: layout::Rect,
    gravity: Gravity,
    mut width: u16,
    mut height: u16,
) -> layout::Rect {
    width = width.min(rect.width);
    height = height.min(rect.height);

    if gravity.contains(Gravity::RIGHT) {
        rect.x = rect.right() - width;
    }
    if gravity.contains(Gravity::BOTTOM) {
        rect.y = rect.bottom() - height;
    }

    layout::Rect { width, height, ..rect }
}

#[must_use]
pub fn rect_fit_inside(mut parent: layout::Rect, mut child: layout::Rect) -> layout::Rect {
    for [start, size] in [
        [|rect| &mut rect.x, |rect| &mut rect.width] as [fn(&mut layout::Rect) -> &mut u16; 2],
        [|rect| &mut rect.y, |rect| &mut rect.height],
    ] {
        let parent_end = *start(&mut parent) + *size(&mut parent);
        let child_end = *start(&mut child) + *size(&mut child);
        if let Some(delta) = child_end.checked_sub(parent_end) {
            start(&mut child).saturating_sub_assign(delta);
        }
        if let Some(delta) = (*start(&mut parent)).checked_sub(*start(&mut child)) {
            *start(&mut child) += delta;
        }
        if let Some(overflow) = (*size(&mut child)).checked_sub(*size(&mut parent)) {
            size(&mut child).saturating_sub_assign(overflow);
        }
    }

    child
}

#[derive(Clone, Copy)]
pub enum Direction {
    Left,
    Right,
    Top,
    Bottom,
}

bitflags::bitflags! {
    #[derive(Clone, Copy)]
    pub struct Gravity: u8 {
        const LEFT = 0;
        const RIGHT = 1;
        const TOP = 0;
        const BOTTOM = 2;
    }
}

impl Gravity {
    #[must_use]
    pub fn move_to(self, dir: Direction) -> Self {
        match dir {
            Direction::Left => self & !Self::RIGHT,
            Direction::Right => self | Self::RIGHT,
            Direction::Top => self & !Self::BOTTOM,
            Direction::Bottom => self | Self::BOTTOM,
        }
    }
}

pub struct AnchoredPosition {
    pub anchor:     Gravity,
    pub x_displace: u16,
    pub y_displace: u16,
}

impl AnchoredPosition {
    #[must_use]
    pub fn to_rect(&self, width: u16, height: u16, parent: layout::Rect) -> layout::Rect {
        let mut rect = layout::Rect {
            x: if self.anchor.contains(Gravity::RIGHT) {
                parent.width.saturating_sub(self.x_displace)
            } else {
                self.x_displace
            },
            y: if self.anchor.contains(Gravity::BOTTOM) {
                parent.height.saturating_sub(self.y_displace)
            } else {
                self.y_displace
            },
            width,
            height,
        };
        if self.anchor.contains(Gravity::RIGHT) {
            rect.x.saturating_sub_assign(width);
        }
        if self.anchor.contains(Gravity::BOTTOM) {
            rect.y.saturating_sub_assign(height);
        }
        rect_fit_inside(parent, rect)
    }

    pub fn move_towards(&mut self, dir: Direction, steps: u16) {
        let (positive_gravity, displace) = match dir {
            Direction::Left | Direction::Right => (Gravity::RIGHT, &mut self.x_displace),
            Direction::Top | Direction::Bottom => (Gravity::BOTTOM, &mut self.y_displace),
        };
        let dir_is_positive = matches!(dir, Direction::Right | Direction::Bottom);
        let anchor_is_positive = self.anchor.contains(positive_gravity);

        if dir_is_positive == anchor_is_positive {
            // move towards displacement source, i.e. subtraction
            displace.saturating_sub_assign(steps);
        } else {
            *displace += steps;
        }
    }

    pub fn anchor_by_nearest(&mut self, width: u16, height: u16, parent: layout::Rect) {
        let child = self.to_rect(width, height, parent);
        #[allow(clippy::type_complexity)]
        let dims: [(fn(layout::Rect) -> u16, fn(layout::Rect) -> u16, _, _); 2] = [
            ((|rect| rect.x), (|rect| rect.width), Gravity::RIGHT, &mut self.x_displace),
            (|rect| rect.y, |rect| rect.height, Gravity::BOTTOM, &mut self.y_displace),
        ];
        for (rect_start, rect_size, positive_gravity, displace) in dims {
            let rect_end = |rect| rect_start(rect) + rect_size(rect);

            let start_margin = rect_start(child).saturating_sub(rect_start(parent));
            let end_margin = rect_end(parent).saturating_sub(rect_end(child));

            if start_margin >= end_margin {
                self.anchor.remove(positive_gravity);
                *displace = start_margin;
            } else {
                self.anchor.insert(positive_gravity);
                *displace = end_margin;
            }
        }
    }
}

pub trait SaturatingAddExt {
    fn saturating_add_assign(&mut self, other: Self);
}

impl<T: SaturatingAdd> SaturatingAddExt for T {
    fn saturating_add_assign(&mut self, other: Self) { *self = self.saturating_add(&other); }
}

pub trait SaturatingSubExt {
    fn saturating_sub_assign(&mut self, other: Self);
}

impl<T: SaturatingSub> SaturatingSubExt for T {
    fn saturating_sub_assign(&mut self, other: Self) { *self = self.saturating_sub(&other); }
}
