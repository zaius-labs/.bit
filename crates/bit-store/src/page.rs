// page.rs — Binary page format for bitstore engine
//
// All integers are little-endian. Pages are 4096 bytes (matching SQLite default).
// Page 0 is the file header. All other pages start with a page_type byte at offset 0.

pub const PAGE_SIZE: usize = 4096;
pub const MAGIC: [u8; 4] = *b"BITS";
pub const FORMAT_VERSION: u32 = 2;

// Page header occupies 11 bytes: [type: u8][page_num: u32][cell_count: u16][extra: u32]
// "extra" is next_leaf for leaf pages, rightmost_child for interior pages.
const PAGE_HEADER_SIZE: usize = 1 + 4 + 2 + 4; // 11 bytes

// ---------------------------------------------------------------------------
// PageError
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum PageError {
    InvalidMagic,
    UnsupportedVersion(u32),
    InvalidPageType(u8),
    PageOverflow,
    CorruptPage(String),
}

impl std::fmt::Display for PageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PageError::InvalidMagic => write!(f, "invalid magic bytes"),
            PageError::UnsupportedVersion(v) => write!(f, "unsupported version: {v}"),
            PageError::InvalidPageType(t) => write!(f, "invalid page type: 0x{t:02x}"),
            PageError::PageOverflow => write!(f, "page overflow"),
            PageError::CorruptPage(msg) => write!(f, "corrupt page: {msg}"),
        }
    }
}

impl std::error::Error for PageError {}

// ---------------------------------------------------------------------------
// PageType
// ---------------------------------------------------------------------------

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageType {
    Header = 0x01,
    BTreeInterior = 0x02,
    BTreeLeaf = 0x03,
    Overflow = 0x04,
    Freelist = 0x05,
}

impl PageType {
    pub fn from_u8(b: u8) -> Result<Self, PageError> {
        match b {
            0x01 => Ok(PageType::Header),
            0x02 => Ok(PageType::BTreeInterior),
            0x03 => Ok(PageType::BTreeLeaf),
            0x04 => Ok(PageType::Overflow),
            0x05 => Ok(PageType::Freelist),
            other => Err(PageError::InvalidPageType(other)),
        }
    }
}

// ---------------------------------------------------------------------------
// File header (page 0)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub magic: [u8; 4],
    pub version: u32,
    pub page_size: u32,
    pub page_count: u32,
    pub freelist_page: u32,
    pub entity_root: u32,
    pub task_root: u32,
    pub flow_root: u32,
    pub schema_root: u32,
    pub blob_root: u32,
    pub change_counter: u64,
}

impl Default for Header {
    fn default() -> Self {
        Self::new()
    }
}

impl Header {
    pub fn new() -> Self {
        Header {
            magic: MAGIC,
            version: FORMAT_VERSION,
            page_size: PAGE_SIZE as u32,
            page_count: 1,
            freelist_page: 0,
            entity_root: 0,
            task_root: 0,
            flow_root: 0,
            schema_root: 0,
            blob_root: 0,
            change_counter: 0,
        }
    }

    pub fn write_to(&self, buf: &mut [u8; PAGE_SIZE]) {
        buf.fill(0);
        let mut off = 0;

        buf[off..off + 4].copy_from_slice(&self.magic);
        off += 4;
        buf[off..off + 4].copy_from_slice(&self.version.to_le_bytes());
        off += 4;
        buf[off..off + 4].copy_from_slice(&self.page_size.to_le_bytes());
        off += 4;
        buf[off..off + 4].copy_from_slice(&self.page_count.to_le_bytes());
        off += 4;
        buf[off..off + 4].copy_from_slice(&self.freelist_page.to_le_bytes());
        off += 4;
        buf[off..off + 4].copy_from_slice(&self.entity_root.to_le_bytes());
        off += 4;
        buf[off..off + 4].copy_from_slice(&self.task_root.to_le_bytes());
        off += 4;
        buf[off..off + 4].copy_from_slice(&self.flow_root.to_le_bytes());
        off += 4;
        buf[off..off + 4].copy_from_slice(&self.schema_root.to_le_bytes());
        off += 4;
        buf[off..off + 4].copy_from_slice(&self.blob_root.to_le_bytes());
        off += 4;
        buf[off..off + 8].copy_from_slice(&self.change_counter.to_le_bytes());
    }

    pub fn read_from(buf: &[u8; PAGE_SIZE]) -> Result<Self, PageError> {
        let magic: [u8; 4] = buf[0..4].try_into().unwrap();
        if magic != MAGIC {
            return Err(PageError::InvalidMagic);
        }

        let version = u32::from_le_bytes(buf[4..8].try_into().unwrap());
        if version != FORMAT_VERSION {
            return Err(PageError::UnsupportedVersion(version));
        }

        let mut off = 8;
        let page_size = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
        off += 4;
        let page_count = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
        off += 4;
        let freelist_page = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
        off += 4;
        let entity_root = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
        off += 4;
        let task_root = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
        off += 4;
        let flow_root = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
        off += 4;
        let schema_root = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
        off += 4;
        let blob_root = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
        off += 4;
        let change_counter = u64::from_le_bytes(buf[off..off + 8].try_into().unwrap());

        Ok(Header {
            magic,
            version,
            page_size,
            page_count,
            freelist_page,
            entity_root,
            task_root,
            flow_root,
            schema_root,
            blob_root,
            change_counter,
        })
    }
}

// ---------------------------------------------------------------------------
// Cell — key-value pair in B-tree nodes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub child_page: u32,
}

impl Cell {
    /// Leaf encoding: [key_len: u16][key][val_len: u32][value]
    pub fn encode_leaf(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.leaf_size());
        out.extend_from_slice(&(self.key.len() as u16).to_le_bytes());
        out.extend_from_slice(&self.key);
        out.extend_from_slice(&(self.value.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.value);
        out
    }

    pub fn decode_leaf(data: &[u8]) -> Result<(Cell, usize), PageError> {
        if data.len() < 2 {
            return Err(PageError::CorruptPage(
                "leaf cell too short for key_len".into(),
            ));
        }
        let key_len = u16::from_le_bytes(data[0..2].try_into().unwrap()) as usize;
        let val_off = 2 + key_len;
        if data.len() < val_off + 4 {
            return Err(PageError::CorruptPage(
                "leaf cell too short for val_len".into(),
            ));
        }
        let key = data[2..val_off].to_vec();
        let val_len = u32::from_le_bytes(data[val_off..val_off + 4].try_into().unwrap()) as usize;
        let val_start = val_off + 4;
        if data.len() < val_start + val_len {
            return Err(PageError::CorruptPage("leaf cell truncated value".into()));
        }
        let value = data[val_start..val_start + val_len].to_vec();
        let consumed = val_start + val_len;
        Ok((
            Cell {
                key,
                value,
                child_page: 0,
            },
            consumed,
        ))
    }

    /// Interior encoding: [key_len: u16][key][child_page: u32]
    pub fn encode_interior(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.interior_size());
        out.extend_from_slice(&(self.key.len() as u16).to_le_bytes());
        out.extend_from_slice(&self.key);
        out.extend_from_slice(&self.child_page.to_le_bytes());
        out
    }

    pub fn decode_interior(data: &[u8]) -> Result<(Cell, usize), PageError> {
        if data.len() < 2 {
            return Err(PageError::CorruptPage(
                "interior cell too short for key_len".into(),
            ));
        }
        let key_len = u16::from_le_bytes(data[0..2].try_into().unwrap()) as usize;
        let child_off = 2 + key_len;
        if data.len() < child_off + 4 {
            return Err(PageError::CorruptPage("interior cell truncated".into()));
        }
        let key = data[2..child_off].to_vec();
        let child_page = u32::from_le_bytes(data[child_off..child_off + 4].try_into().unwrap());
        let consumed = child_off + 4;
        Ok((
            Cell {
                key,
                value: Vec::new(),
                child_page,
            },
            consumed,
        ))
    }

    pub fn leaf_size(&self) -> usize {
        2 + self.key.len() + 4 + self.value.len()
    }

    pub fn interior_size(&self) -> usize {
        2 + self.key.len() + 4
    }
}

// ---------------------------------------------------------------------------
// LeafPage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LeafPage {
    pub page_num: u32,
    pub cells: Vec<Cell>,
    pub next_leaf: u32,
}

impl LeafPage {
    pub fn new(page_num: u32) -> Self {
        LeafPage {
            page_num,
            cells: Vec::new(),
            next_leaf: 0,
        }
    }

    /// Layout: [page_type: u8 = 0x03][page_num: u32][cell_count: u16][next_leaf: u32][cells...]
    pub fn write_to(&self, buf: &mut [u8; PAGE_SIZE]) -> Result<(), PageError> {
        let total = PAGE_HEADER_SIZE + self.used_space();
        if total > PAGE_SIZE {
            return Err(PageError::PageOverflow);
        }

        buf.fill(0);
        let mut off = 0;

        buf[off] = PageType::BTreeLeaf as u8;
        off += 1;
        buf[off..off + 4].copy_from_slice(&self.page_num.to_le_bytes());
        off += 4;
        buf[off..off + 2].copy_from_slice(&(self.cells.len() as u16).to_le_bytes());
        off += 2;
        buf[off..off + 4].copy_from_slice(&self.next_leaf.to_le_bytes());
        off += 4;

        for cell in &self.cells {
            let encoded = cell.encode_leaf();
            buf[off..off + encoded.len()].copy_from_slice(&encoded);
            off += encoded.len();
        }

        Ok(())
    }

    pub fn read_from(buf: &[u8; PAGE_SIZE]) -> Result<Self, PageError> {
        let ptype = buf[0];
        if ptype != PageType::BTreeLeaf as u8 {
            return Err(PageError::InvalidPageType(ptype));
        }

        let page_num = u32::from_le_bytes(buf[1..5].try_into().unwrap());
        let cell_count = u16::from_le_bytes(buf[5..7].try_into().unwrap()) as usize;
        let next_leaf = u32::from_le_bytes(buf[7..11].try_into().unwrap());

        let mut off = PAGE_HEADER_SIZE;
        let mut cells = Vec::with_capacity(cell_count);
        for _ in 0..cell_count {
            let (cell, consumed) = Cell::decode_leaf(&buf[off..])?;
            off += consumed;
            cells.push(cell);
        }

        Ok(LeafPage {
            page_num,
            cells,
            next_leaf,
        })
    }

    pub fn used_space(&self) -> usize {
        self.cells.iter().map(|c| c.leaf_size()).sum()
    }

    pub fn can_fit(&self, cell: &Cell) -> bool {
        PAGE_HEADER_SIZE + self.used_space() + cell.leaf_size() <= PAGE_SIZE
    }
}

// ---------------------------------------------------------------------------
// InteriorPage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct InteriorPage {
    pub page_num: u32,
    pub cells: Vec<Cell>,
    pub rightmost_child: u32,
}

impl InteriorPage {
    pub fn new(page_num: u32) -> Self {
        InteriorPage {
            page_num,
            cells: Vec::new(),
            rightmost_child: 0,
        }
    }

    /// Layout: [page_type: u8 = 0x02][page_num: u32][cell_count: u16][rightmost_child: u32][cells...]
    pub fn write_to(&self, buf: &mut [u8; PAGE_SIZE]) -> Result<(), PageError> {
        let total: usize =
            PAGE_HEADER_SIZE + self.cells.iter().map(|c| c.interior_size()).sum::<usize>();
        if total > PAGE_SIZE {
            return Err(PageError::PageOverflow);
        }

        buf.fill(0);
        let mut off = 0;

        buf[off] = PageType::BTreeInterior as u8;
        off += 1;
        buf[off..off + 4].copy_from_slice(&self.page_num.to_le_bytes());
        off += 4;
        buf[off..off + 2].copy_from_slice(&(self.cells.len() as u16).to_le_bytes());
        off += 2;
        buf[off..off + 4].copy_from_slice(&self.rightmost_child.to_le_bytes());
        off += 4;

        for cell in &self.cells {
            let encoded = cell.encode_interior();
            buf[off..off + encoded.len()].copy_from_slice(&encoded);
            off += encoded.len();
        }

        Ok(())
    }

    pub fn read_from(buf: &[u8; PAGE_SIZE]) -> Result<Self, PageError> {
        let ptype = buf[0];
        if ptype != PageType::BTreeInterior as u8 {
            return Err(PageError::InvalidPageType(ptype));
        }

        let page_num = u32::from_le_bytes(buf[1..5].try_into().unwrap());
        let cell_count = u16::from_le_bytes(buf[5..7].try_into().unwrap()) as usize;
        let rightmost_child = u32::from_le_bytes(buf[7..11].try_into().unwrap());

        let mut off = PAGE_HEADER_SIZE;
        let mut cells = Vec::with_capacity(cell_count);
        for _ in 0..cell_count {
            let (cell, consumed) = Cell::decode_interior(&buf[off..])?;
            off += consumed;
            cells.push(cell);
        }

        Ok(InteriorPage {
            page_num,
            cells,
            rightmost_child,
        })
    }

    pub fn can_fit(&self, cell: &Cell) -> bool {
        let used: usize = self.cells.iter().map(|c| c.interior_size()).sum();
        PAGE_HEADER_SIZE + used + cell.interior_size() <= PAGE_SIZE
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cell(key: &[u8], value: &[u8], child_page: u32) -> Cell {
        Cell {
            key: key.to_vec(),
            value: value.to_vec(),
            child_page,
        }
    }

    #[test]
    fn header_roundtrip() {
        let mut h = Header::new();
        h.page_count = 42;
        h.entity_root = 7;
        h.change_counter = 999;

        let mut buf = [0u8; PAGE_SIZE];
        h.write_to(&mut buf);
        let h2 = Header::read_from(&buf).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn header_validates_magic() {
        let mut buf = [0u8; PAGE_SIZE];
        Header::new().write_to(&mut buf);
        buf[0] = b'X';
        match Header::read_from(&buf) {
            Err(PageError::InvalidMagic) => {}
            other => panic!("expected InvalidMagic, got {:?}", other),
        }
    }

    #[test]
    fn header_validates_version() {
        let mut buf = [0u8; PAGE_SIZE];
        Header::new().write_to(&mut buf);
        buf[4..8].copy_from_slice(&99u32.to_le_bytes());
        match Header::read_from(&buf) {
            Err(PageError::UnsupportedVersion(99)) => {}
            other => panic!("expected UnsupportedVersion(99), got {:?}", other),
        }
    }

    #[test]
    fn cell_leaf_roundtrip() {
        let cell = make_cell(b"hello", b"world", 0);
        let encoded = cell.encode_leaf();
        let (decoded, consumed) = Cell::decode_leaf(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded.key, cell.key);
        assert_eq!(decoded.value, cell.value);
        assert_eq!(decoded.child_page, 0);
    }

    #[test]
    fn cell_interior_roundtrip() {
        let cell = make_cell(b"key123", b"", 42);
        let encoded = cell.encode_interior();
        let (decoded, consumed) = Cell::decode_interior(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded.key, cell.key);
        assert_eq!(decoded.child_page, 42);
        assert!(decoded.value.is_empty());
    }

    #[test]
    fn cell_with_empty_value() {
        let cell = make_cell(b"k", b"", 0);
        let encoded = cell.encode_leaf();
        let (decoded, _) = Cell::decode_leaf(&encoded).unwrap();
        assert_eq!(decoded.key, b"k");
        assert!(decoded.value.is_empty());
    }

    #[test]
    fn cell_with_large_key() {
        let big_key = vec![0xAB; 1000];
        let cell = Cell {
            key: big_key.clone(),
            value: b"val".to_vec(),
            child_page: 0,
        };
        let encoded = cell.encode_leaf();
        let (decoded, consumed) = Cell::decode_leaf(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded.key, big_key);
        assert_eq!(decoded.value, b"val");
    }

    #[test]
    fn leaf_page_roundtrip() {
        let mut page = LeafPage::new(5);
        page.next_leaf = 6;
        for i in 0..5 {
            page.cells.push(make_cell(
                format!("key{i}").as_bytes(),
                format!("value{i}").as_bytes(),
                0,
            ));
        }

        let mut buf = [0u8; PAGE_SIZE];
        page.write_to(&mut buf).unwrap();
        let page2 = LeafPage::read_from(&buf).unwrap();

        assert_eq!(page2.page_num, 5);
        assert_eq!(page2.next_leaf, 6);
        assert_eq!(page2.cells.len(), 5);
        for i in 0..5 {
            assert_eq!(page2.cells[i].key, format!("key{i}").as_bytes());
            assert_eq!(page2.cells[i].value, format!("value{i}").as_bytes());
        }
    }

    #[test]
    fn leaf_page_overflow() {
        let mut page = LeafPage::new(1);
        // Each cell: 2 + 10 + 4 + 4000 = 4016 bytes. One cell + 11-byte header = 4027, fits.
        // Two cells would be 4016*2 + 11 = 8043, way over.
        page.cells.push(make_cell(&[0; 10], &[0; 4000], 0));
        let mut buf = [0u8; PAGE_SIZE];
        page.write_to(&mut buf).unwrap(); // first cell fits

        page.cells.push(make_cell(&[0; 10], &[0; 4000], 0));
        match page.write_to(&mut buf) {
            Err(PageError::PageOverflow) => {}
            other => panic!("expected PageOverflow, got {:?}", other),
        }
    }

    #[test]
    fn interior_page_roundtrip() {
        let mut page = InteriorPage::new(10);
        page.rightmost_child = 99;
        for i in 0..3 {
            page.cells
                .push(make_cell(format!("k{i}").as_bytes(), b"", (i + 1) as u32));
        }

        let mut buf = [0u8; PAGE_SIZE];
        page.write_to(&mut buf).unwrap();
        let page2 = InteriorPage::read_from(&buf).unwrap();

        assert_eq!(page2.page_num, 10);
        assert_eq!(page2.rightmost_child, 99);
        assert_eq!(page2.cells.len(), 3);
        for i in 0..3 {
            assert_eq!(page2.cells[i].key, format!("k{i}").as_bytes());
            assert_eq!(page2.cells[i].child_page, (i + 1) as u32);
        }
    }

    #[test]
    fn leaf_page_empty() {
        let page = LeafPage::new(0);
        let mut buf = [0u8; PAGE_SIZE];
        page.write_to(&mut buf).unwrap();
        let page2 = LeafPage::read_from(&buf).unwrap();
        assert_eq!(page2.page_num, 0);
        assert!(page2.cells.is_empty());
        assert_eq!(page2.next_leaf, 0);
    }

    #[test]
    fn can_fit_returns_false_when_full() {
        let mut page = LeafPage::new(1);
        // Fill with cells until can_fit returns false
        let cell = make_cell(b"0123456789", &[0xFF; 200], 0);
        while page.can_fit(&cell) {
            page.cells.push(cell.clone());
        }
        assert!(!page.can_fit(&cell));
        // But we should still be able to write what we have
        let mut buf = [0u8; PAGE_SIZE];
        page.write_to(&mut buf).unwrap();
    }
}
