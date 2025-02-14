// We initialize the heap across 2 different regions of DRAM to access the maximum number of bytes we can reasonably allocate. This is done due to the large amount of heap memory this application needs. The WiFi stack takes the first 64 kB of the first DRAM region and we need to keep 16k spare for the stack. The second region has some spare, but it's better to operate according to additions of factors of 2 (in this case 64+32)

use core::mem::MaybeUninit;

use esp_alloc as _;

pub fn init_heap() {
    const HEAP_SIZE: usize = 98767;
    #[link_section = ".dram2_uninit"]
    static mut HEAP: MaybeUninit<[u8; HEAP_SIZE]> = MaybeUninit::uninit();

    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            HEAP.as_mut_ptr() as *mut u8,
            HEAP_SIZE,
            esp_alloc::MemoryCapability::Internal.into(),
        ));
    }
}
