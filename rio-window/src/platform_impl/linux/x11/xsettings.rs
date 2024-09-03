//! Parser for the xsettings data format.
//!
//! Some of this code is referenced from [here].
//!
//! [here]: https://github.com/derat/xsettingsd

use std::iter;
use std::num::NonZeroUsize;

use x11rb::protocol::xproto::{self, ConnectionExt};

use super::atoms::*;
use super::XConnection;

type Result<T> = core::result::Result<T, ParserError>;

const DPI_NAME: &[u8] = b"Xft/DPI";
const DPI_MULTIPLIER: f64 = 1024.0;
const LITTLE_ENDIAN: u8 = b'l';
const BIG_ENDIAN: u8 = b'B';

impl XConnection {
    /// Get the DPI from XSettings.
    pub(crate) fn xsettings_dpi(
        &self,
        xsettings_screen: xproto::Atom,
    ) -> core::result::Result<Option<f64>, super::X11Error> {
        let atoms = self.atoms();

        // Get the current owner of the screen's settings.
        let owner = self
            .xcb_connection()
            .get_selection_owner(xsettings_screen)?
            .reply()?;

        // Read the _XSETTINGS_SETTINGS property.
        let data: Vec<u8> = self.get_property(
            owner.owner,
            atoms[_XSETTINGS_SETTINGS],
            atoms[_XSETTINGS_SETTINGS],
        )?;

        // Parse the property.
        let dpi_setting = read_settings(&data)?
            .find(|res| res.as_ref().map_or(true, |s| s.name == DPI_NAME))
            .transpose()?;
        if let Some(dpi_setting) = dpi_setting {
            let base_dpi = match dpi_setting.data {
                SettingData::Integer(dpi) => dpi as f64,
                SettingData::String(_) => {
                    return Err(ParserError::BadType(SettingType::String).into())
                }
                SettingData::Color(_) => {
                    return Err(ParserError::BadType(SettingType::Color).into())
                }
            };

            Ok(Some(base_dpi / DPI_MULTIPLIER))
        } else {
            Ok(None)
        }
    }
}

/// Read over the settings in the block of data.
fn read_settings(data: &[u8]) -> Result<impl Iterator<Item = Result<Setting<'_>>> + '_> {
    // Create a parser. This automatically parses the first 8 bytes for metadata.
    let mut parser = Parser::new(data)?;

    // Read the total number of settings.
    let total_settings = parser.i32()?;

    // Iterate over the settings.
    let iter = iter::repeat_with(move || Setting::parse(&mut parser))
        .take(total_settings as usize);
    Ok(iter)
}

/// A setting in the settings list.
struct Setting<'a> {
    /// The name of the setting.
    name: &'a [u8],

    /// The data contained in the setting.
    data: SettingData<'a>,
}

/// The data contained in a setting.
enum SettingData<'a> {
    Integer(i32),
    String(#[allow(dead_code)] &'a [u8]),
    Color(#[allow(dead_code)] [i16; 4]),
}

impl<'a> Setting<'a> {
    /// Parse a new `SettingData`.
    fn parse(parser: &mut Parser<'a>) -> Result<Self> {
        // Read the type.
        let ty: SettingType = parser.i8()?.try_into()?;

        // Read another byte of padding.
        parser.advance(1)?;

        // Read the name of the setting.
        let name_len = parser.i16()?;
        let name = parser.advance(name_len as usize)?;
        parser.pad(name.len(), 4)?;

        // Ignore the serial number.
        parser.advance(4)?;

        let data = match ty {
            SettingType::Integer => {
                // Read a 32-bit integer.
                SettingData::Integer(parser.i32()?)
            }

            SettingType::String => {
                // Read the data.
                let data_len = parser.i32()?;
                let data = parser.advance(data_len as usize)?;
                parser.pad(data.len(), 4)?;

                SettingData::String(data)
            }

            SettingType::Color => {
                // Read i16's of color.
                let (red, blue, green, alpha) =
                    (parser.i16()?, parser.i16()?, parser.i16()?, parser.i16()?);

                SettingData::Color([red, blue, green, alpha])
            }
        };

        Ok(Setting { name, data })
    }
}

#[derive(Debug)]
pub enum SettingType {
    Integer = 0,
    String = 1,
    Color = 2,
}

impl TryFrom<i8> for SettingType {
    type Error = ParserError;

    fn try_from(value: i8) -> Result<Self> {
        Ok(match value {
            0 => Self::Integer,
            1 => Self::String,
            2 => Self::Color,
            x => return Err(ParserError::InvalidType(x)),
        })
    }
}

/// Parser for the incoming byte stream.
struct Parser<'a> {
    bytes: &'a [u8],
    endianness: Endianness,
}

impl<'a> Parser<'a> {
    /// Create a new parser.
    fn new(bytes: &'a [u8]) -> Result<Self> {
        let (endianness, bytes) = bytes
            .split_first()
            .ok_or_else(|| ParserError::ran_out(1, 0))?;
        let endianness = match *endianness {
            BIG_ENDIAN => Endianness::Big,
            LITTLE_ENDIAN => Endianness::Little,
            _ => Endianness::native(),
        };

        Ok(Self {
            // Ignore three bytes of padding and the four-byte serial.
            bytes: bytes
                .get(7..)
                .ok_or_else(|| ParserError::ran_out(7, bytes.len()))?,
            endianness,
        })
    }

    /// Get a slice of bytes.
    fn advance(&mut self, n: usize) -> Result<&'a [u8]> {
        if n == 0 {
            return Ok(&[]);
        }

        if n > self.bytes.len() {
            Err(ParserError::ran_out(n, self.bytes.len()))
        } else {
            let (part, rem) = self.bytes.split_at(n);
            self.bytes = rem;
            Ok(part)
        }
    }

    /// Skip some padding.
    fn pad(&mut self, size: usize, pad: usize) -> Result<()> {
        let advance = (pad - (size % pad)) % pad;
        self.advance(advance)?;
        Ok(())
    }

    /// Get a single byte.
    fn i8(&mut self) -> Result<i8> {
        self.advance(1).map(|s| s[0] as i8)
    }

    /// Get two bytes.
    fn i16(&mut self) -> Result<i16> {
        self.advance(2).map(|s| {
            let bytes: &[u8; 2] = s.try_into().unwrap();
            match self.endianness {
                Endianness::Big => i16::from_be_bytes(*bytes),
                Endianness::Little => i16::from_le_bytes(*bytes),
            }
        })
    }

    /// Get four bytes.
    fn i32(&mut self) -> Result<i32> {
        self.advance(4).map(|s| {
            let bytes: &[u8; 4] = s.try_into().unwrap();
            match self.endianness {
                Endianness::Big => i32::from_be_bytes(*bytes),
                Endianness::Little => i32::from_le_bytes(*bytes),
            }
        })
    }
}

/// Endianness of the incoming data.
enum Endianness {
    Little,
    Big,
}

impl Endianness {
    #[cfg(target_endian = "little")]
    fn native() -> Self {
        Endianness::Little
    }

    #[cfg(target_endian = "big")]
    fn native() -> Self {
        Endianness::Big
    }
}

/// Parser errors.
#[allow(dead_code)]
#[derive(Debug)]
pub enum ParserError {
    /// Ran out of bytes.
    NoMoreBytes {
        expected: NonZeroUsize,
        found: usize,
    },

    /// Invalid type.
    InvalidType(i8),

    /// Bad setting type.
    BadType(SettingType),
}

impl ParserError {
    fn ran_out(expected: usize, found: usize) -> ParserError {
        let expected = NonZeroUsize::new(expected).unwrap();
        Self::NoMoreBytes { expected, found }
    }
}

#[cfg(test)]
/// Tests for the XSETTINGS parser.
mod tests {
    use super::*;

    const XSETTINGS: &str = include_str!("tests/xsettings.dat");

    #[test]
    fn empty() {
        let err = match read_settings(&[]) {
            Ok(_) => panic!(),
            Err(err) => err,
        };
        match err {
            ParserError::NoMoreBytes { expected, found } => {
                assert_eq!(expected.get(), 1);
                assert_eq!(found, 0);
            }

            _ => panic!(),
        }
    }

    #[test]
    fn parse_xsettings() {
        let data = XSETTINGS
            .trim()
            .split(',')
            .map(|tok| {
                let val = tok.strip_prefix("0x").unwrap();
                u8::from_str_radix(val, 16).unwrap()
            })
            .collect::<Vec<_>>();

        let settings = read_settings(&data)
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap();

        let dpi = settings.iter().find(|s| s.name == b"Xft/DPI").unwrap();
        assert_int(&dpi.data, 96 * 1024);
        let hinting = settings.iter().find(|s| s.name == b"Xft/Hinting").unwrap();
        assert_int(&hinting.data, 1);

        let rgba = settings.iter().find(|s| s.name == b"Xft/RGBA").unwrap();
        assert_string(&rgba.data, "rgb");
        let lcd = settings
            .iter()
            .find(|s| s.name == b"Xft/Lcdfilter")
            .unwrap();
        assert_string(&lcd.data, "lcddefault");
    }

    fn assert_string(dat: &SettingData<'_>, s: &str) {
        match dat {
            SettingData::String(left) => assert_eq!(*left, s.as_bytes()),
            _ => panic!("invalid data type"),
        }
    }

    fn assert_int(dat: &SettingData<'_>, i: i32) {
        match dat {
            SettingData::Integer(left) => assert_eq!(*left, i),
            _ => panic!("invalid data type"),
        }
    }
}
