//! A camera model's declared abilities (identity + capabilities), as recorded
//! in libgphoto2's driver database.

use gphoto2_sys as sys;

use crate::util::fixed_buf_to_string;

/// The capabilities and identity of a specific camera model.
#[derive(Clone)]
pub struct Abilities {
    raw: sys::CameraAbilities,
}

impl Abilities {
    pub(crate) fn from_raw(raw: sys::CameraAbilities) -> Self {
        Self { raw }
    }

    /// Human-readable model name (e.g. `"Canon EOS 5D Mark IV"`).
    pub fn model(&self) -> String {
        fixed_buf_to_string(&self.raw.model)
    }

    /// Maturity of the driver backing this model.
    pub fn driver_status(&self) -> DriverStatus {
        DriverStatus::from_raw(self.raw.status)
    }

    /// USB vendor ID, or `0` if not a USB device / unspecified.
    pub fn usb_vendor_id(&self) -> u16 {
        self.raw.usb_vendor as u16
    }

    /// USB product ID, or `0` if not a USB device / unspecified.
    pub fn usb_product_id(&self) -> u16 {
        self.raw.usb_product as u16
    }

    /// Whether the model can be driven over USB.
    pub fn supports_usb(&self) -> bool {
        self.raw.port.0 & sys::GPPortType::GP_PORT_USB.0 != 0
    }

    /// Whether the model can be driven over a serial port.
    pub fn supports_serial(&self) -> bool {
        self.raw.port.0 & sys::GPPortType::GP_PORT_SERIAL.0 != 0
    }

    /// Access the underlying raw [`sys::CameraAbilities`] for fields not yet
    /// surfaced by this wrapper.
    pub fn as_raw(&self) -> &sys::CameraAbilities {
        &self.raw
    }
}

impl core::fmt::Debug for Abilities {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Abilities")
            .field("model", &self.model())
            .field("driver_status", &self.driver_status())
            .field(
                "usb_vendor_id",
                &format_args!("{:#06x}", self.usb_vendor_id()),
            )
            .field(
                "usb_product_id",
                &format_args!("{:#06x}", self.usb_product_id()),
            )
            .finish()
    }
}

/// Driver maturity, mirroring `CameraDriverStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverStatus {
    /// Production ready (`GP_DRIVER_STATUS_PRODUCTION`).
    Production,
    /// Beta quality (`GP_DRIVER_STATUS_TESTING`).
    Testing,
    /// Alpha quality, may not work (`GP_DRIVER_STATUS_EXPERIMENTAL`).
    Experimental,
    /// Slated for removal (`GP_DRIVER_STATUS_DEPRECATED`).
    Deprecated,
    /// An unrecognised status value.
    Unknown,
}

impl DriverStatus {
    fn from_raw(status: sys::CameraDriverStatus) -> Self {
        // Match on the integer payload to avoid relying on derived structural
        // equality of the bindgen newtype.
        match status.0 {
            0 => Self::Production,   // GP_DRIVER_STATUS_PRODUCTION
            1 => Self::Testing,      // GP_DRIVER_STATUS_TESTING
            2 => Self::Experimental, // GP_DRIVER_STATUS_EXPERIMENTAL
            3 => Self::Deprecated,   // GP_DRIVER_STATUS_DEPRECATED
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_status_mapping() {
        assert_eq!(
            DriverStatus::from_raw(sys::CameraDriverStatus(0)),
            DriverStatus::Production
        );
        assert_eq!(
            DriverStatus::from_raw(sys::CameraDriverStatus(3)),
            DriverStatus::Deprecated
        );
        // Anything outside the known set is reported as Unknown, not a panic.
        assert_eq!(
            DriverStatus::from_raw(sys::CameraDriverStatus(99)),
            DriverStatus::Unknown
        );
    }

    #[test]
    fn port_flags_are_decoded_from_the_bitmask() {
        let mut raw: sys::CameraAbilities = unsafe { core::mem::zeroed() };
        raw.model[0] = b'X' as core::ffi::c_char;
        raw.port =
            sys::GPPortType(sys::GPPortType::GP_PORT_USB.0 | sys::GPPortType::GP_PORT_SERIAL.0);
        let both = Abilities::from_raw(raw);
        assert!(both.supports_usb());
        assert!(both.supports_serial());
        assert_eq!(both.model(), "X");

        let mut usb_only: sys::CameraAbilities = unsafe { core::mem::zeroed() };
        usb_only.port = sys::GPPortType::GP_PORT_USB;
        let usb = Abilities::from_raw(usb_only);
        assert!(usb.supports_usb());
        assert!(!usb.supports_serial());
    }
}
