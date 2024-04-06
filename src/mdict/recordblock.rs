use std::io::prelude::*;
use std::io::Read;

use flate2::read::ZlibDecoder;
use nom::bytes::complete::take;
use nom::combinator::map;
use nom::IResult;
use nom::multi::count;
use nom::number::complete::{be_u32, be_u64, le_u32};
use nom::sequence::tuple;
use ripemd::{Digest, Ripemd128};
use salsa20::{cipher::KeyIvInit, Salsa20};

use crate::mdict::header::{Header, Version};
use crate::util::fast_decrypt;

/// every record block compressed size and decompressed size
#[derive(Debug)]
pub struct RecordBlockSize {
    pub csize: usize,
    pub dsize: usize,
}

pub fn parse_record_blocks<'a>(
    data: &'a [u8],
    header: &'a Header,
) -> IResult<&'a [u8], Vec<RecordBlockSize>> {
    match &header.version {
        Version::V1 => parse_record_blocks_v1(data),
        Version::V2 => parse_record_blocks_v2(data),
    }
}

fn parse_record_blocks_v1(data: &[u8]) -> IResult<&[u8], Vec<RecordBlockSize>> {
    let (data, (records_num, _entries_num, record_info_len, _record_buf_len)) =
        tuple((be_u32, be_u32, be_u32, be_u32))(data)?;

    assert_eq!(records_num * 8, record_info_len);

    count(
        map(tuple((be_u32, be_u32)), |(csize, dsize)| RecordBlockSize {
            csize: csize as usize,
            dsize: dsize as usize,
        }),
        records_num as usize,
    )(data)
}

fn parse_record_blocks_v2(data: &[u8]) -> IResult<&[u8], Vec<RecordBlockSize>> {
    let (data, (records_num, _entries_num, record_info_len, _record_buf_len)) =
        tuple((be_u64, be_u64, be_u64, be_u64))(data)?;

    assert_eq!(records_num * 16, record_info_len,);

    count(
        map(tuple((be_u64, be_u64)), |(csize, dsize)| RecordBlockSize {
            csize: csize as usize,
            dsize: dsize as usize,
        }),
        records_num as usize,
    )(data)
}

// todo: pub vs pub(crate) diff
pub(crate) fn record_block_parser<'a>(
    size: usize,
    dsize: usize,
) -> impl FnMut(&'a [u8]) -> IResult<&'a [u8], Vec<u8>> {
    map(
        tuple((le_u32, take(4_usize), take(size - 8))),
        move |(enc, checksum, encrypted)| {
            let enc_method = (enc >> 4) & 0xf;
            let enc_size = (enc >> 8) & 0xff;
            let comp_method = enc & 0xf;

            let mut md = Ripemd128::new();
            md.update(checksum);
            let key = md.finalize();

            let data: Vec<u8> = match enc_method {
                0 => Vec::from(encrypted),
                1 => fast_decrypt(encrypted, key.as_slice()),
                2 => {
                    let mut decrypt = vec![];
                    let mut cipher = Salsa20::new(key.as_slice().into(), &[0; 8].into());
                    decrypt
                }
                _ => panic!("unknown enc method: {}", enc_method),
            };

            let decompressed = match comp_method {
                0 => data,
                1 => {
                    let lzo = minilzo_rs::LZO::init().unwrap();
                    lzo.decompress(&data[..], dsize).unwrap()
                }
                2 => {
                    let mut v = vec![];
                    ZlibDecoder::new(&data[..]).read_to_end(&mut v).unwrap();
                    v
                }
                _ => panic!("unknown compression method: {}", comp_method),
            };

            decompressed
        },
    )
}
