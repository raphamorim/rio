#[repr(transparent)]
#[derive(PartialEq, PartialOrd, Debug, Clone, Copy)]
pub struct NSAppKitVersion(f64);

#[allow(dead_code)]
#[allow(non_upper_case_globals)]
impl NSAppKitVersion {
    pub fn current() -> Self {
        extern "C" {
            static NSAppKitVersionNumber: NSAppKitVersion;
        }

        unsafe { NSAppKitVersionNumber }
    }

    pub fn floor(self) -> Self {
        Self(self.0.floor())
    }

    pub const NSAppKitVersionNumber10_0: Self = Self(577.0);
    pub const NSAppKitVersionNumber10_1: Self = Self(620.0);
    pub const NSAppKitVersionNumber10_2: Self = Self(663.0);
    pub const NSAppKitVersionNumber10_2_3: Self = Self(663.6);
    pub const NSAppKitVersionNumber10_3: Self = Self(743.0);
    pub const NSAppKitVersionNumber10_3_2: Self = Self(743.14);
    pub const NSAppKitVersionNumber10_3_3: Self = Self(743.2);
    pub const NSAppKitVersionNumber10_3_5: Self = Self(743.24);
    pub const NSAppKitVersionNumber10_3_7: Self = Self(743.33);
    pub const NSAppKitVersionNumber10_3_9: Self = Self(743.36);
    pub const NSAppKitVersionNumber10_4: Self = Self(824.0);
    pub const NSAppKitVersionNumber10_4_1: Self = Self(824.1);
    pub const NSAppKitVersionNumber10_4_3: Self = Self(824.23);
    pub const NSAppKitVersionNumber10_4_4: Self = Self(824.33);
    pub const NSAppKitVersionNumber10_4_7: Self = Self(824.41);
    pub const NSAppKitVersionNumber10_5: Self = Self(949.0);
    pub const NSAppKitVersionNumber10_5_2: Self = Self(949.27);
    pub const NSAppKitVersionNumber10_5_3: Self = Self(949.33);
    pub const NSAppKitVersionNumber10_6: Self = Self(1038.0);
    pub const NSAppKitVersionNumber10_7: Self = Self(1138.0);
    pub const NSAppKitVersionNumber10_7_2: Self = Self(1138.23);
    pub const NSAppKitVersionNumber10_7_3: Self = Self(1138.32);
    pub const NSAppKitVersionNumber10_7_4: Self = Self(1138.47);
    pub const NSAppKitVersionNumber10_8: Self = Self(1187.0);
    pub const NSAppKitVersionNumber10_9: Self = Self(1265.0);
    pub const NSAppKitVersionNumber10_10: Self = Self(1343.0);
    pub const NSAppKitVersionNumber10_10_2: Self = Self(1344.0);
    pub const NSAppKitVersionNumber10_10_3: Self = Self(1347.0);
    pub const NSAppKitVersionNumber10_10_4: Self = Self(1348.0);
    pub const NSAppKitVersionNumber10_10_5: Self = Self(1348.0);
    pub const NSAppKitVersionNumber10_10_Max: Self = Self(1349.0);
    pub const NSAppKitVersionNumber10_11: Self = Self(1404.0);
    pub const NSAppKitVersionNumber10_11_1: Self = Self(1404.13);
    pub const NSAppKitVersionNumber10_11_2: Self = Self(1404.34);
    pub const NSAppKitVersionNumber10_11_3: Self = Self(1404.34);
    pub const NSAppKitVersionNumber10_12: Self = Self(1504.0);
    pub const NSAppKitVersionNumber10_12_1: Self = Self(1504.60);
    pub const NSAppKitVersionNumber10_12_2: Self = Self(1504.76);
    pub const NSAppKitVersionNumber10_13: Self = Self(1561.0);
    pub const NSAppKitVersionNumber10_13_1: Self = Self(1561.1);
    pub const NSAppKitVersionNumber10_13_2: Self = Self(1561.2);
    pub const NSAppKitVersionNumber10_13_4: Self = Self(1561.4);
}
