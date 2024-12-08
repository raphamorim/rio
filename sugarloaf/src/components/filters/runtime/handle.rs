// This file was originally taken from https://github.com/SnowflakePowered/librashader
// SnowflakePowered/librashader is licensed under MPL-2.0
// https://github.com/SnowflakePowered/librashader/blob/master/LICENSE.md

use std::ops::Deref;

pub enum Handle<'a, T> {
    Borrowed(&'a T),
    Owned(T),
}

impl<T> Deref for Handle<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Handle::Borrowed(r) => r,
            Handle::Owned(r) => r,
        }
    }
}
