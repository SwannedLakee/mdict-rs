use crate::mdict::header::parse_header;
use crate::mdict::keyblock::{
    Entry, parse_key_block_header, parse_key_block_info, parse_key_blocks,
};
use crate::mdict::recordblock::{parse_record_blocks, record_block_parser, RecordBlockSize};

/// 一个record的定位信息：在buf中的offset和在block解压后的offset
/// draw with: https://asciiflow.com/#/
//                   ◄──block_csize───►
//                   ┌────────────────┐
//            block  │                │
//                   └────────────────┘
//                   ▲
//                buf_offset
//
//                   ◄──── block_dsize ───────►
//                   ┌───┬────────────┬───────┐
//     block_decomp  │   │   record   │       │
//                   └───┴────────────┴───────┘
//                       ▲
//                 block_decompressed_offset
//
#[derive(Debug)]
pub struct RecordOffset {
    pub(crate) text: String,
    // record所在block在buf的offset 截取block使用
    block_start_in_buf: usize,
    // 解析block使用
    block_csize: usize,
    block_dsize: usize,
    // record在解压后的block的offset
    record_start_in_de_block: usize,
    record_end_in_de_block: usize,
}

// todo: why can not be String?
#[derive(Debug)]
pub struct Record<'a> {
    pub(crate) text: &'a str,
    pub(crate) definition: String,
}

/// MDX 详细结构见 https://bitbucket.org/xwang/mdict-analysis/src/master/MDX.svg
/// MDX file 结构
/// header: 得到 version encoding encrypted
/// key block header: entry number and checksum
/// key block size info: every key block compressed and decompressed size, for parse key block bytes
/// key block bytes: 根据上面的key block info得到的（csize,dsize）解析得到 Entry list
/// record header: record block size, entry number, record block info size, record block size
/// record block size info: every record block compressed and decompressed size, 用于解析下面的record block
/// record block bytes: entry and definition bytes, parsed by RecordEntry and RecordBlockSize
/// entry: 是一个索引
/// record: 是一条释义
#[derive(Debug)]
pub struct Mdx {
    pub records_offset: Vec<RecordOffset>,
    pub record_block_buf: Vec<u8>,
    pub encoding: String,
    pub encrypted: String,
}

impl Mdx {
    /// let data = include_bytes!("/file.mdx");
    /// let mdx = Mdx::new(&data);
    pub fn new(data: &[u8]) -> Mdx {
        let (data, header) = parse_header(data).unwrap();

        let (data, kbh) = parse_key_block_header(data, &header).unwrap();
        let (data, key_blocks_size) =
            parse_key_block_info(data, kbh.key_block_info_len, &header).unwrap();
        let (data, entries) =
            parse_key_blocks(data, kbh.key_blocks_len, &header, &key_blocks_size).unwrap();
        let (data, record_blocks_size) = parse_record_blocks(data, &header).unwrap();

        //计算position耗时，一次计算就保存下来
        let offset: Vec<RecordOffset> = records_offset(&entries, &record_blocks_size);

        Mdx {
            records_offset: offset,
            record_block_buf: Vec::from(data),
            encoding: header.encoding,
            encrypted: header.encrypted,
        }
    }

    #[allow(unused)]
    pub fn entries(&self) -> impl Iterator<Item=&RecordOffset> {
        return self.records_offset.iter();
    }

    pub fn items(&self) -> impl Iterator<Item=Record> {
        self.records_offset.iter().map(|rs| {
            let def = self.find_definition(&rs);
            Record {
                text: &rs.text,
                definition: def,
            }
        })
    }

    fn find_definition(&self, rs: &RecordOffset) -> String {
        // block bytes with tail
        let block_buf = &self.record_block_buf[rs.block_start_in_buf..];

        let (_, block_decompressed) =
            record_block_parser(rs.block_csize, rs.block_dsize)(block_buf).unwrap();

        let record_decompressed =
            &block_decompressed[rs.record_start_in_de_block..rs.record_end_in_de_block];

        let def = String::from_utf8_lossy(record_decompressed).to_string();

        return def;
    }
}

/// bytes structure: buf -> block -> record(entry)
fn records_offset(
    entries: &Vec<Entry>,
    record_blocks_size: &Vec<RecordBlockSize>,
) -> Vec<RecordOffset> {
    let mut positions: Vec<RecordOffset> = vec![];
    let mut i: usize = 0;
    let mut pre_blocks_dsize_sum = 0;
    let mut pre_blocks_csize_sum = 0;
    // 同时开始遍历record_blocks_size和entries，每个block包含0或n个entry，当entry的buf_decompressed_offset > pre_blocks_dsize_sum时 说明当前block已经遍历
    for block in record_blocks_size {
        while i < entries.len() {
            let entry = &entries[i];

            // 当前entry已经属于下一个block，注意等于号
            if entry.record_start_in_de_buf >= pre_blocks_dsize_sum + block.dsize {
                break;
            }

            let mut record_end_in_de_block = 0;
            if i < entries.len() - 1 {
                // 计算 record_end_in_decomp_block
                let next_entry = &entries[i + 1];
                record_end_in_de_block =
                    next_entry.record_start_in_de_buf - pre_blocks_dsize_sum;
            } else {
                // last entry
                record_end_in_de_block = block.dsize
            }

            positions.push(RecordOffset {
                text: entry.text.to_string(),
                block_start_in_buf: pre_blocks_csize_sum,
                block_csize: block.csize,
                block_dsize: block.dsize,
                record_start_in_de_block: entry.record_start_in_de_buf - pre_blocks_dsize_sum,
                record_end_in_de_block,
            });
            i += 1;
        }
        pre_blocks_dsize_sum += block.dsize;
        pre_blocks_csize_sum += block.csize;
    }
    return positions;
}
