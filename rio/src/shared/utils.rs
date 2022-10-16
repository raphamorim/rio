// #[inline]
// pub fn convert_to_utf8_string(byte: u8) -> String {
//     let vec_byte: Vec<u8> = vec![byte];
//     match String::from_utf8(vec_byte) {
//         Ok(s) => s,
//         Err(_e) => {
//             // Todo: Enable option of utf8_lossy and log
//             // println!("{:?}", d);
//             // String::from_utf8_lossy(&vec_byte).to_string()
//             String::from("?")
//         }
//     }
// }
