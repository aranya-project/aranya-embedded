use core::mem::{size_of_val, MaybeUninit};

pub fn init_heap() {
    #[link_section = ".dram2_uninit"]
    static mut HEAP2: MaybeUninit<[u8; 64 * 1024]> = MaybeUninit::uninit();

    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            HEAP2.as_mut_ptr() as *mut u8,
            size_of_val(&*core::ptr::addr_of!(HEAP2)),
            esp_alloc::MemoryCapability::Internal.into(),
        ));
    }
}
