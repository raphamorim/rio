## Conversion Enums

```rust
pub enum Format {
    SRGB0_255,
    SRGB0_1,
}
```

Enums are based on color conversion rules:

```bash
Hex #FFFFFF

sRGB   0-255 =  255.000  255.000  255.000
sRGB   0-1.0 =  1.00000  1.00000  1.00000
RGB Adobe 98 =  255.000  255.000  255.000
```

## Conversion to WGPU Color

[WGPU Color](https://docs.rs/wgpu/latest/wgpu/struct.Color.html)

```rust
let color: wgpu::Color = ColorBuilder::from_hex(String::from("#151515"), Format::SRGB0_255).unwrap().to_wgpu();
assert_eq!(
    color,
    Color {
        r: 21.0,
        g: 21.0,
        b: 21.0,
        a: 1.0
    }
);
```
