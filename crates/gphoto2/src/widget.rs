//! Camera configuration as a tree of [`Widget`]s.
//!
//! Drivers expose their settings (ISO, aperture, capture target, date/time, …)
//! as a tree: a top-level `Window` containing `Section`s containing leaf
//! controls (`Text`, `Range`, `Toggle`, `Radio`/`Menu`, `Date`). Read the tree
//! with [`Camera::config`](crate::Camera::config), change leaf values, then push
//! the whole tree back with [`Camera::set_config`](crate::Camera::set_config).
//!
//! # Ownership
//!
//! libgphoto2 frees a widget tree from its root and ignores per-node
//! reference counts, and child/lookup accessors return *borrowed* pointers. So
//! every [`Widget`] handle here shares ownership of the whole tree (via `Rc`):
//! the tree is freed exactly once, when the last handle drops. Mutating any
//! handle mutates the shared tree, so changes are visible from sibling handles
//! and are applied when you call `set_config` with any handle.

use core::ffi::{c_char, c_int};
use std::ffi::CString;
use std::rc::Rc;

use gphoto2_sys as sys;

use crate::error::{check, Error, Result};
use crate::util::cstr_to_string;

/// Owns the root of a widget tree; frees the entire tree on drop.
struct WidgetTree {
    root: *mut sys::CameraWidget,
}

impl Drop for WidgetTree {
    fn drop(&mut self) {
        // The root reached us with ref_count 1; unref'ing to 0 frees the whole
        // tree (children are owned by the tree and never freed individually).
        unsafe { sys::gp_widget_unref(self.root) };
    }
}

/// A node in a camera's configuration tree. See the [module docs](self).
#[derive(Clone)]
pub struct Widget {
    node: *mut sys::CameraWidget,
    tree: Rc<WidgetTree>,
}

impl Widget {
    /// Wrap a freshly obtained, caller-owned root widget (ref_count 1).
    pub(crate) fn from_root(root: *mut sys::CameraWidget) -> Self {
        Self {
            node: root,
            tree: Rc::new(WidgetTree { root }),
        }
    }

    /// A handle to another node in the same tree (shares ownership).
    fn sibling(&self, node: *mut sys::CameraWidget) -> Self {
        Self {
            node,
            tree: Rc::clone(&self.tree),
        }
    }

    pub(crate) fn as_ptr(&self) -> *mut sys::CameraWidget {
        self.node
    }

    /// The root of this node's tree (what `set_config` must be given).
    pub(crate) fn root_ptr(&self) -> *mut sys::CameraWidget {
        self.tree.root
    }

    /// The internal name — a stable identifier, e.g. `"iso"`.
    pub fn name(&self) -> Result<String> {
        let mut p: *const c_char = core::ptr::null();
        check(unsafe { sys::gp_widget_get_name(self.node, &mut p) })?;
        Ok(cstr_to_string(p))
    }

    /// The human-readable label, e.g. `"ISO Speed"`.
    pub fn label(&self) -> Result<String> {
        let mut p: *const c_char = core::ptr::null();
        check(unsafe { sys::gp_widget_get_label(self.node, &mut p) })?;
        Ok(cstr_to_string(p))
    }

    /// Help/info text (often empty).
    pub fn info(&self) -> Result<String> {
        let mut p: *const c_char = core::ptr::null();
        check(unsafe { sys::gp_widget_get_info(self.node, &mut p) })?;
        Ok(cstr_to_string(p))
    }

    /// Whether the value is read-only (informational, not user-settable).
    pub fn readonly(&self) -> Result<bool> {
        let mut v: c_int = 0;
        check(unsafe { sys::gp_widget_get_readonly(self.node, &mut v) })?;
        Ok(v != 0)
    }

    /// This widget's type, which determines the shape of its value.
    pub fn widget_type(&self) -> Result<WidgetType> {
        let mut t = sys::CameraWidgetType(0);
        check(unsafe { sys::gp_widget_get_type(self.node, &mut t) })?;
        Ok(WidgetType::from_raw(t))
    }

    /// Number of direct children.
    pub fn child_count(&self) -> Result<usize> {
        Ok(check(unsafe { sys::gp_widget_count_children(self.node) })? as usize)
    }

    /// This widget's direct children (empty for leaf widgets).
    pub fn children(&self) -> Result<Vec<Widget>> {
        let count = self.child_count()?;
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let mut child: *mut sys::CameraWidget = core::ptr::null_mut();
            check(unsafe { sys::gp_widget_get_child(self.node, i as c_int, &mut child) })?;
            out.push(self.sibling(child));
        }
        Ok(out)
    }

    /// Find a descendant by its internal name. Searches recursively, so calling
    /// this on the root finds any setting in the tree.
    pub fn get_child(&self, name: &str) -> Result<Widget> {
        let cname = to_cstring(name)?;
        let mut child: *mut sys::CameraWidget = core::ptr::null_mut();
        check(unsafe { sys::gp_widget_get_child_by_name(self.node, cname.as_ptr(), &mut child) })?;
        Ok(self.sibling(child))
    }

    /// The choices of a `Radio`/`Menu` widget (empty for other types).
    pub fn choices(&self) -> Result<Vec<String>> {
        let count = check(unsafe { sys::gp_widget_count_choices(self.node) })?;
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            let mut p: *const c_char = core::ptr::null();
            check(unsafe { sys::gp_widget_get_choice(self.node, i, &mut p) })?;
            out.push(cstr_to_string(p));
        }
        Ok(out)
    }

    /// `(min, max, increment)` for a `Range` widget.
    pub fn range(&self) -> Result<(f32, f32, f32)> {
        let (mut min, mut max, mut step) = (0.0f32, 0.0f32, 0.0f32);
        check(unsafe { sys::gp_widget_get_range(self.node, &mut min, &mut max, &mut step) })?;
        Ok((min, max, step))
    }

    /// Read this widget's value. The returned variant matches
    /// [`Widget::widget_type`]; containers and buttons yield [`WidgetValue::None`].
    pub fn value(&self) -> Result<WidgetValue> {
        Ok(match self.widget_type()? {
            WidgetType::Text => WidgetValue::Text(self.read_string()?),
            WidgetType::Radio | WidgetType::Menu => WidgetValue::Choice(self.read_string()?),
            WidgetType::Range => {
                let mut v = 0.0f32;
                check(unsafe { sys::gp_widget_get_value(self.node, (&mut v as *mut f32).cast()) })?;
                WidgetValue::Range(v)
            }
            WidgetType::Toggle => WidgetValue::Toggle(self.read_int()? != 0),
            WidgetType::Date => WidgetValue::Date(self.read_int()?),
            WidgetType::Window | WidgetType::Section | WidgetType::Button | WidgetType::Unknown => {
                WidgetValue::None
            }
        })
    }

    /// Set this widget's value. The variant must match the widget's type.
    pub fn set_value(&self, value: &WidgetValue) -> Result<()> {
        match value {
            WidgetValue::Text(s) | WidgetValue::Choice(s) => {
                let c = to_cstring(s)?;
                check(unsafe { sys::gp_widget_set_value(self.node, c.as_ptr().cast()) })?;
            }
            WidgetValue::Range(v) => {
                check(unsafe { sys::gp_widget_set_value(self.node, (v as *const f32).cast()) })?;
            }
            WidgetValue::Toggle(on) => {
                let v: c_int = i32::from(*on);
                check(unsafe { sys::gp_widget_set_value(self.node, (&v as *const c_int).cast()) })?;
            }
            WidgetValue::Date(secs) => {
                let v: c_int = *secs;
                check(unsafe { sys::gp_widget_set_value(self.node, (&v as *const c_int).cast()) })?;
            }
            WidgetValue::None => return Err(Error::new(sys::GP_ERROR_BAD_PARAMETERS)),
        }
        Ok(())
    }

    /// Convenience: set a `Text` widget's value.
    pub fn set_text(&self, value: &str) -> Result<()> {
        self.set_value(&WidgetValue::Text(value.to_owned()))
    }

    /// Convenience: select a choice on a `Radio`/`Menu` widget.
    pub fn set_choice(&self, choice: &str) -> Result<()> {
        self.set_value(&WidgetValue::Choice(choice.to_owned()))
    }

    /// Convenience: set a `Toggle` widget.
    pub fn set_toggle(&self, on: bool) -> Result<()> {
        self.set_value(&WidgetValue::Toggle(on))
    }

    /// Convenience: set a `Range` widget.
    pub fn set_range(&self, value: f32) -> Result<()> {
        self.set_value(&WidgetValue::Range(value))
    }

    fn read_string(&self) -> Result<String> {
        let mut p: *const c_char = core::ptr::null();
        check(unsafe {
            sys::gp_widget_get_value(self.node, (&mut p as *mut *const c_char).cast())
        })?;
        Ok(if p.is_null() {
            String::new()
        } else {
            cstr_to_string(p)
        })
    }

    fn read_int(&self) -> Result<i32> {
        let mut v: c_int = 0;
        check(unsafe { sys::gp_widget_get_value(self.node, (&mut v as *mut c_int).cast()) })?;
        Ok(v)
    }
}

impl core::fmt::Debug for Widget {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Widget")
            .field("name", &self.name().unwrap_or_default())
            .field("type", &self.widget_type().ok())
            .finish()
    }
}

/// The type of a [`Widget`], mirroring `CameraWidgetType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetType {
    /// Top-level configuration container.
    Window,
    /// A grouping of settings (think "tab").
    Section,
    /// Free-form text value.
    Text,
    /// Numeric slider with a min/max/increment (see [`Widget::range`]).
    Range,
    /// On/off checkbox.
    Toggle,
    /// One-of-N selection (see [`Widget::choices`]).
    Radio,
    /// One-of-N selection, same value shape as `Radio`.
    Menu,
    /// A button that triggers a driver action (no readable value here).
    Button,
    /// Date/time value, expressed as seconds since the Unix epoch.
    Date,
    /// An unrecognised widget type.
    Unknown,
}

impl WidgetType {
    fn from_raw(t: sys::CameraWidgetType) -> Self {
        match t.0 {
            0 => Self::Window,
            1 => Self::Section,
            2 => Self::Text,
            3 => Self::Range,
            4 => Self::Toggle,
            5 => Self::Radio,
            6 => Self::Menu,
            7 => Self::Button,
            8 => Self::Date,
            _ => Self::Unknown,
        }
    }
}

/// A typed configuration value, paired with [`Widget::value`] /
/// [`Widget::set_value`].
#[derive(Debug, Clone, PartialEq)]
pub enum WidgetValue {
    /// `Text` widget value.
    Text(String),
    /// Selected choice of a `Radio`/`Menu` widget.
    Choice(String),
    /// `Range` widget value.
    Range(f32),
    /// `Toggle` widget value.
    Toggle(bool),
    /// `Date` widget value: seconds since the Unix epoch.
    Date(i32),
    /// No value (containers and buttons).
    None,
}

fn to_cstring(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| Error::new(sys::GP_ERROR_BAD_PARAMETERS))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a small config tree by hand via the C API, then wrap it. This
    /// exercises the whole widget wrapper without needing a camera.
    fn sample_tree() -> Widget {
        unsafe {
            let new = |ty: sys::CameraWidgetType, label: &str| -> *mut sys::CameraWidget {
                let mut w: *mut sys::CameraWidget = core::ptr::null_mut();
                let label = CString::new(label).unwrap();
                assert!(sys::gp_widget_new(ty, label.as_ptr(), &mut w) >= 0);
                w
            };
            let set_name = |w: *mut sys::CameraWidget, name: &str| {
                let n = CString::new(name).unwrap();
                assert!(sys::gp_widget_set_name(w, n.as_ptr()) >= 0);
            };

            let root = new(sys::CameraWidgetType::GP_WIDGET_WINDOW, "config");

            let text = new(sys::CameraWidgetType::GP_WIDGET_TEXT, "Owner Name");
            set_name(text, "owner");
            let tval = CString::new("Ansel").unwrap();
            assert!(sys::gp_widget_set_value(text, tval.as_ptr().cast()) >= 0);
            assert!(sys::gp_widget_append(root, text) >= 0);

            let radio = new(sys::CameraWidgetType::GP_WIDGET_RADIO, "ISO Speed");
            set_name(radio, "iso");
            for choice in ["100", "200", "400"] {
                let c = CString::new(choice).unwrap();
                assert!(sys::gp_widget_add_choice(radio, c.as_ptr()) >= 0);
            }
            let rval = CString::new("200").unwrap();
            assert!(sys::gp_widget_set_value(radio, rval.as_ptr().cast()) >= 0);
            assert!(sys::gp_widget_append(root, radio) >= 0);

            let toggle = new(sys::CameraWidgetType::GP_WIDGET_TOGGLE, "Some Flag");
            set_name(toggle, "flag");
            let on: c_int = 1;
            assert!(sys::gp_widget_set_value(toggle, (&on as *const c_int).cast()) >= 0);
            assert!(sys::gp_widget_append(root, toggle) >= 0);

            Widget::from_root(root)
        }
    }

    #[test]
    fn walks_tree_and_reads_typed_values() {
        let root = sample_tree();
        assert_eq!(root.widget_type().unwrap(), WidgetType::Window);
        assert_eq!(root.child_count().unwrap(), 3);

        let names: Vec<String> = root
            .children()
            .unwrap()
            .iter()
            .map(|c| c.name().unwrap())
            .collect();
        assert_eq!(names, vec!["owner", "iso", "flag"]);

        // Recursive lookup by name from the root.
        let iso = root.get_child("iso").unwrap();
        assert_eq!(iso.widget_type().unwrap(), WidgetType::Radio);
        assert_eq!(iso.label().unwrap(), "ISO Speed");
        assert_eq!(iso.choices().unwrap(), vec!["100", "200", "400"]);
        assert_eq!(iso.value().unwrap(), WidgetValue::Choice("200".into()));

        assert_eq!(
            root.get_child("owner").unwrap().value().unwrap(),
            WidgetValue::Text("Ansel".into())
        );
        assert_eq!(
            root.get_child("flag").unwrap().value().unwrap(),
            WidgetValue::Toggle(true)
        );
    }

    #[test]
    fn set_value_mutates_the_shared_tree() {
        let root = sample_tree();

        root.get_child("iso").unwrap().set_choice("400").unwrap();
        // A different handle to the same node observes the change.
        assert_eq!(
            root.get_child("iso").unwrap().value().unwrap(),
            WidgetValue::Choice("400".into())
        );

        root.get_child("owner")
            .unwrap()
            .set_text("Dorothea")
            .unwrap();
        assert_eq!(
            root.get_child("owner").unwrap().value().unwrap(),
            WidgetValue::Text("Dorothea".into())
        );

        root.get_child("flag").unwrap().set_toggle(false).unwrap();
        assert_eq!(
            root.get_child("flag").unwrap().value().unwrap(),
            WidgetValue::Toggle(false)
        );
    }

    #[test]
    fn setting_a_type_mismatch_is_rejected() {
        let root = sample_tree();
        // The root window holds no value.
        assert!(root.set_value(&WidgetValue::None).is_err());
    }

    #[test]
    fn child_handle_outlives_root_handle() {
        // Shared ownership: dropping the root handle must not free the tree
        // while a child handle is alive.
        let child = {
            let root = sample_tree();
            root.get_child("iso").unwrap()
        };
        assert_eq!(child.name().unwrap(), "iso");
        assert_eq!(child.choices().unwrap().len(), 3);
    }
}
