use core::mem::{size_of_val, MaybeUninit};

// We initialize the heap across 2 different regions of DRAM to access the maximum number of bytes we can reasonably allocate. This is done due to the large amount of heap memory this application needs. The WiFi stack takes the first 64 kB of the first DRAM region and we need to keep 16k spare for the stack. The second region has some spare, but it's better to operate according to additions of factors of 2 (in this case 64+32)

pub fn init_heap() {
    // Main DRAM heap region (16k spare for stack)
    #[link_section = ".dram_uninit"]
    static mut HEAP: MaybeUninit<[u8; 96 * 1024]> = MaybeUninit::uninit();

    // Secondary DRAM heap region (2k spare)
    #[link_section = ".dram2_uninit"]
    static mut HEAP2: MaybeUninit<[u8; 96 * 1024]> = MaybeUninit::uninit();

    unsafe {
        // Add main DRAM region
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            HEAP.as_mut_ptr() as *mut u8,
            size_of_val(&*core::ptr::addr_of!(HEAP)),
            esp_alloc::MemoryCapability::Internal.into(),
        ));

        // Add secondary DRAM region
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            HEAP2.as_mut_ptr() as *mut u8,
            size_of_val(&*core::ptr::addr_of!(HEAP2)),
            esp_alloc::MemoryCapability::Internal.into(),
        ));
    }
}
