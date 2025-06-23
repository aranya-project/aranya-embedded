# parameter-store

The parameter store library serializes and deserializes a `Parameters` struct to
abstracted I/O implementations. It currently has implementations for
`embedded_storage::Storage` on `no_std` targets and `std::fs::File` on `std`
targets.
