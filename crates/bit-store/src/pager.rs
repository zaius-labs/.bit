// pager.rs — I/O engine for bitstore
//
// Manages reading/writing 4KB pages to/from a .bitstore file.
// Everything above the Pager thinks in pages, never in raw file bytes or offsets.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::page::{Header, PageError, PageType, PAGE_SIZE};

pub type PageBuf = [u8; PAGE_SIZE];

#[derive(Debug)]
pub enum PagerError {
    Io(std::io::Error),
    Page(PageError),
    InvalidPageNum(u32),
}

impl std::fmt::Display for PagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PagerError::Io(e) => write!(f, "pager I/O error: {e}"),
            PagerError::Page(e) => write!(f, "pager page error: {e}"),
            PagerError::InvalidPageNum(n) => write!(f, "invalid page number: {n}"),
        }
    }
}

impl std::error::Error for PagerError {}

impl From<std::io::Error> for PagerError {
    fn from(e: std::io::Error) -> Self {
        PagerError::Io(e)
    }
}

impl From<PageError> for PagerError {
    fn from(e: PageError) -> Self {
        PagerError::Page(e)
    }
}

struct CachedPage {
    data: Box<PageBuf>,
    dirty: bool,
}

pub struct Pager {
    file: File,
    header: Header,
    cache: HashMap<u32, CachedPage>,
}

impl Pager {
    /// Create a new .bitstore file. Writes the header page.
    pub fn create(path: &Path) -> Result<Self, PagerError> {
        let mut file = File::create(path)?;
        let header = Header::new();

        let mut buf = [0u8; PAGE_SIZE];
        header.write_to(&mut buf);
        file.write_all(&buf)?;
        file.sync_all()?;

        // Reopen for read+write
        let file = OpenOptions::new().read(true).write(true).open(path)?;

        Ok(Pager {
            file,
            header,
            cache: HashMap::new(),
        })
    }

    /// Open an existing .bitstore file. Reads and validates the header.
    pub fn open(path: &Path) -> Result<Self, PagerError> {
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;

        let mut buf = [0u8; PAGE_SIZE];
        file.read_exact(&mut buf)?;
        let header = Header::read_from(&buf)?;

        Ok(Pager {
            file,
            header,
            cache: HashMap::new(),
        })
    }

    /// Get a reference to the file header.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Get a mutable reference to the file header.
    pub fn header_mut(&mut self) -> &mut Header {
        &mut self.header
    }

    /// Read page N. Returns cached copy if available, otherwise reads from file.
    pub fn read_page(&mut self, page_num: u32) -> Result<&PageBuf, PagerError> {
        if page_num >= self.header.page_count {
            return Err(PagerError::InvalidPageNum(page_num));
        }

        if !self.cache.contains_key(&page_num) {
            let offset = page_num as u64 * PAGE_SIZE as u64;
            self.file.seek(SeekFrom::Start(offset))?;
            let mut data = Box::new([0u8; PAGE_SIZE]);
            self.file.read_exact(data.as_mut())?;
            self.cache
                .insert(page_num, CachedPage { data, dirty: false });
        }

        Ok(&self.cache[&page_num].data)
    }

    /// Write data to page N. Writes to cache, marks dirty.
    pub fn write_page(&mut self, page_num: u32, data: PageBuf) -> Result<(), PagerError> {
        if page_num >= self.header.page_count {
            return Err(PagerError::InvalidPageNum(page_num));
        }

        self.cache.insert(
            page_num,
            CachedPage {
                data: Box::new(data),
                dirty: true,
            },
        );

        Ok(())
    }

    /// Allocate a new page. Checks freelist first, otherwise extends file.
    /// Returns the page number of the newly allocated page.
    pub fn allocate(&mut self) -> Result<u32, PagerError> {
        if self.header.freelist_page != 0 {
            let free_page_num = self.header.freelist_page;

            // Read the freelist page to get the next pointer
            let page_data = self.read_page(free_page_num)?;
            // Freelist format: [page_type: 0x05][next_free: u32][rest: unused]
            let next_free = u32::from_le_bytes(page_data[1..5].try_into().unwrap());

            self.header.freelist_page = next_free;

            // Remove from cache (it's being repurposed)
            self.cache.remove(&free_page_num);

            Ok(free_page_num)
        } else {
            let new_page = self.header.page_count;
            self.header.page_count += 1;
            Ok(new_page)
        }
    }

    /// Free a page (return to freelist).
    pub fn free(&mut self, page_num: u32) -> Result<(), PagerError> {
        let mut buf = [0u8; PAGE_SIZE];
        buf[0] = PageType::Freelist as u8;
        buf[1..5].copy_from_slice(&self.header.freelist_page.to_le_bytes());

        self.write_page(page_num, buf)?;
        self.header.freelist_page = page_num;

        Ok(())
    }

    /// Flush all dirty pages to disk. Also writes the header.
    pub fn flush(&mut self) -> Result<(), PagerError> {
        self.header.change_counter += 1;

        // Write header to page 0
        let mut header_buf = [0u8; PAGE_SIZE];
        self.header.write_to(&mut header_buf);
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header_buf)?;

        // Write all dirty pages
        let dirty_pages: Vec<u32> = self
            .cache
            .iter()
            .filter(|(_, cp)| cp.dirty)
            .map(|(&num, _)| num)
            .collect();

        for page_num in dirty_pages {
            let offset = page_num as u64 * PAGE_SIZE as u64;
            self.file.seek(SeekFrom::Start(offset))?;
            self.file.write_all(self.cache[&page_num].data.as_ref())?;
            self.cache.get_mut(&page_num).unwrap().dirty = false;
        }

        self.file.sync_all()?;

        Ok(())
    }

    /// Number of pages in the file.
    pub fn page_count(&self) -> u32 {
        self.header.page_count
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn tmp_path() -> NamedTempFile {
        NamedTempFile::new().unwrap()
    }

    #[test]
    fn create_and_reopen() {
        let tmp = tmp_path();
        let path = tmp.path();

        {
            let mut pager = Pager::create(path).unwrap();
            assert_eq!(pager.header().page_count, 1);
            assert_eq!(pager.header().magic, *b"BITS");
            assert_eq!(pager.header().version, 2);
            pager.flush().unwrap();
        }

        {
            let pager = Pager::open(path).unwrap();
            assert_eq!(pager.header().page_count, 1);
            assert_eq!(pager.header().magic, *b"BITS");
            assert_eq!(pager.header().version, 2);
            assert_eq!(pager.header().change_counter, 1);
        }
    }

    #[test]
    fn allocate_sequential() {
        let tmp = tmp_path();
        let mut pager = Pager::create(tmp.path()).unwrap();

        let p1 = pager.allocate().unwrap();
        let p2 = pager.allocate().unwrap();
        let p3 = pager.allocate().unwrap();

        assert_eq!(p1, 1);
        assert_eq!(p2, 2);
        assert_eq!(p3, 3);
        assert_eq!(pager.page_count(), 4);
    }

    #[test]
    fn write_read_page() {
        let tmp = tmp_path();
        let mut pager = Pager::create(tmp.path()).unwrap();

        let page_num = pager.allocate().unwrap();
        let mut data = [0u8; PAGE_SIZE];
        data[0] = 0xAB;
        data[100] = 0xCD;
        data[4095] = 0xEF;
        pager.write_page(page_num, data).unwrap();

        let read_back = pager.read_page(page_num).unwrap();
        assert_eq!(read_back[0], 0xAB);
        assert_eq!(read_back[100], 0xCD);
        assert_eq!(read_back[4095], 0xEF);
    }

    #[test]
    fn cache_hit() {
        let tmp = tmp_path();
        let mut pager = Pager::create(tmp.path()).unwrap();

        let page_num = pager.allocate().unwrap();
        let mut data = [0u8; PAGE_SIZE];
        data[0] = 0x42;
        pager.write_page(page_num, data).unwrap();

        // Read without flush — should come from cache
        let read_back = pager.read_page(page_num).unwrap();
        assert_eq!(read_back[0], 0x42);
    }

    #[test]
    fn flush_persists() {
        let tmp = tmp_path();
        let path = tmp.path().to_path_buf();

        {
            let mut pager = Pager::create(&path).unwrap();
            let page_num = pager.allocate().unwrap();
            assert_eq!(page_num, 1);

            let mut data = [0u8; PAGE_SIZE];
            data[0] = 0x99;
            data[42] = 0x77;
            pager.write_page(page_num, data).unwrap();
            pager.flush().unwrap();
        }

        {
            let mut pager = Pager::open(&path).unwrap();
            let read_back = pager.read_page(1).unwrap();
            assert_eq!(read_back[0], 0x99);
            assert_eq!(read_back[42], 0x77);
        }
    }

    #[test]
    fn freelist_basic() {
        let tmp = tmp_path();
        let mut pager = Pager::create(tmp.path()).unwrap();

        let p1 = pager.allocate().unwrap();
        assert_eq!(p1, 1);

        pager.free(p1).unwrap();
        assert_eq!(pager.header().freelist_page, 1);

        let p1_again = pager.allocate().unwrap();
        assert_eq!(p1_again, 1);
        assert_eq!(pager.header().freelist_page, 0);
    }

    #[test]
    fn freelist_chain() {
        let tmp = tmp_path();
        let mut pager = Pager::create(tmp.path()).unwrap();

        let p1 = pager.allocate().unwrap();
        let p2 = pager.allocate().unwrap();
        let p3 = pager.allocate().unwrap();
        assert_eq!((p1, p2, p3), (1, 2, 3));

        // Free all three — builds a chain
        pager.free(p1).unwrap();
        pager.free(p2).unwrap();
        pager.free(p3).unwrap();

        // LIFO: should get 3, 2, 1
        let a = pager.allocate().unwrap();
        let b = pager.allocate().unwrap();
        let c = pager.allocate().unwrap();
        assert_eq!((a, b, c), (3, 2, 1));
    }

    #[test]
    fn allocate_extends_file() {
        let tmp = tmp_path();
        let mut pager = Pager::create(tmp.path()).unwrap();
        assert_eq!(pager.page_count(), 1);

        let p = pager.allocate().unwrap();
        assert_eq!(p, 1);
        assert_eq!(pager.page_count(), 2);
    }

    #[test]
    fn invalid_page_num() {
        let tmp = tmp_path();
        let mut pager = Pager::create(tmp.path()).unwrap();
        // Only page 0 exists (the header)
        let _ = pager.allocate().unwrap(); // page 1 now exists

        match pager.read_page(9999) {
            Err(PagerError::InvalidPageNum(9999)) => {}
            other => panic!("expected InvalidPageNum(9999), got {:?}", other),
        }
    }

    #[test]
    fn change_counter_increments() {
        let tmp = tmp_path();
        let mut pager = Pager::create(tmp.path()).unwrap();
        assert_eq!(pager.header().change_counter, 0);

        pager.flush().unwrap();
        assert_eq!(pager.header().change_counter, 1);

        pager.flush().unwrap();
        assert_eq!(pager.header().change_counter, 2);
    }
}
