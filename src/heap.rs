use core::mem::MaybeUninit;

pub fn init_heap() {
    #[link_section = ".dram2_uninit"]
    static mut HEAP2: core::mem::MaybeUninit<[u8; 64 * 1024]> = core::mem::MaybeUninit::uninit();

    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            HEAP2.as_mut_ptr() as *mut u8,
            core::mem::size_of_val(&*core::ptr::addr_of!(HEAP2)),
            esp_alloc::MemoryCapability::Internal.into(),
        ));
    }
}
