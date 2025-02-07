use crate::engine::EmbeddedCommand;
use aranya_runtime::{
    Address, Checkpoint, Command, CommandId, Fact, FactIndex, FactPerspective, GraphId, Keys, Location, Perspective, PolicyId, Prior, Query, QueryMut, Revertable, Segment, Storage, StorageError, StorageProvider
};

pub struct EmbeddedPerspective;

impl Query for EmbeddedPerspective {
    fn query(&self, name: &str, keys: &[Box<[u8]>]) -> Result<Option<Box<[u8]>>, StorageError> {
        Err(StorageError::NoSuchStorage)
    }

    type QueryIterator = Box<dyn Iterator<Item = Result<Fact, StorageError>>>;

    fn query_prefix(
        &self,
        name: &str,
        prefix: &[Box<[u8]>],
    ) -> Result<Self::QueryIterator, StorageError> {
        Err(StorageError::NoSuchStorage)
    }
}

impl QueryMut for EmbeddedPerspective {
    fn insert(&mut self, name: String, keys: Keys, value: Box<[u8]>) {}

    fn delete(&mut self, name: String, keys: Keys) {}
}

impl FactPerspective for EmbeddedPerspective {}

impl Perspective for EmbeddedPerspective {
    fn policy(&self) -> PolicyId {
        PolicyId::new(0)
    }

    fn add_command(&mut self, command: &impl Command) -> Result<usize, StorageError> {
        Ok(0)
    }

    fn includes(&self, id: CommandId) -> bool {
        false
    }

    fn head_address(&self) -> Result<Prior<Address>, buggy::Bug> {
        buggy::bug!("error")
    }
}

impl Revertable for EmbeddedPerspective {
    fn checkpoint(&self) -> Checkpoint {
        Checkpoint { index: 0 }
    }

    fn revert(&mut self, checkpoint: Checkpoint) -> Result<(), buggy::Bug> {
        Ok(())
    }
}

pub struct EmbeddedFactIndex;

impl Query for EmbeddedFactIndex {
    fn query(&self, name: &str, keys: &[Box<[u8]>]) -> Result<Option<Box<[u8]>>, StorageError> {
        Err(StorageError::NoSuchStorage)
    }

    type QueryIterator = Box<dyn Iterator<Item = Result<Fact, StorageError>>>;

    fn query_prefix(
        &self,
        name: &str,
        prefix: &[Box<[u8]>],
    ) -> Result<Self::QueryIterator, StorageError> {
        Err(StorageError::NoSuchStorage)
    }
}

impl FactIndex for EmbeddedFactIndex {}

pub struct EmbeddedSegment;

impl Segment for EmbeddedSegment {
    type FactIndex = EmbeddedFactIndex;
    type Command<'a> = EmbeddedCommand;

    fn head(&self) -> Result<Self::Command<'_>, StorageError> {
        Err(StorageError::NoSuchStorage)
    }

    fn first(&self) -> Self::Command<'_> {
        EmbeddedCommand
    }

    fn head_location(&self) -> Location {
        Location::new(0, 0)
    }

    fn first_location(&self) -> Location {
        Location::new(0, 0)
    }

    fn contains(&self, location: Location) -> bool {
        false
    }

    fn policy(&self) -> PolicyId {
        PolicyId::new(0)
    }

    fn prior(&self) -> Prior<Location> {
        Prior::None
    }

    fn get_command(&self, location: Location) -> Option<Self::Command<'_>> {
        None
    }

    fn get_from_max_cut(&self, max_cut: usize) -> Result<Option<Location>, StorageError> {
        Err(StorageError::NoSuchStorage)
    }

    fn get_from(&self, location: Location) -> Vec<Self::Command<'_>> {
        Vec::new()
    }

    fn facts(&self) -> Result<Self::FactIndex, StorageError> {
        Err(StorageError::NoSuchStorage)
    }

    fn shortest_max_cut(&self) -> usize {
        0
    }

    fn longest_max_cut(&self) -> Result<usize, StorageError> {
        Err(StorageError::NoSuchStorage)
    }

    fn skip_list(&self) -> &[(Location, usize)] {
        &[]
    }
}

pub struct EmbeddedFactPerspective;

impl Query for EmbeddedFactPerspective {
    fn query(&self, name: &str, keys: &[Box<[u8]>]) -> Result<Option<Box<[u8]>>, StorageError> {
        Err(StorageError::NoSuchStorage)
    }

    type QueryIterator = Box<dyn Iterator<Item = Result<Fact, StorageError>>>;

    fn query_prefix(
        &self,
        name: &str,
        prefix: &[Box<[u8]>],
    ) -> Result<Self::QueryIterator, StorageError> {
        Err(StorageError::NoSuchStorage)
    }
}

impl QueryMut for EmbeddedFactPerspective {
    fn insert(&mut self, name: String, keys: Keys, value: Box<[u8]>) {}

    fn delete(&mut self, name: String, keys: Keys) {}
}

impl FactPerspective for EmbeddedFactPerspective {

}

pub struct EmbeddedStorage;

impl Storage for EmbeddedStorage {
    type Perspective = EmbeddedPerspective;
    type FactPerspective = EmbeddedFactPerspective;
    type Segment = EmbeddedSegment;
    type FactIndex = EmbeddedFactIndex;

    fn get_command_id(&self, location: Location) -> Result<CommandId, StorageError> {
        Err(StorageError::NoSuchStorage)
    }

    fn get_linear_perspective(
            &self,
            parent: Location,
        ) -> Result<Option<Self::Perspective>, StorageError> {
        Ok(None)
    }

    fn get_fact_perspective(&self, first: Location) -> Result<Self::FactPerspective, StorageError> {
        Err(StorageError::NoSuchStorage)
    }

    fn new_merge_perspective(
            &self,
            left: Location,
            right: Location,
            last_common_ancestor: (Location, usize),
            policy_id: PolicyId,
            braid: Self::FactIndex,
        ) -> Result<Option<Self::Perspective>, StorageError> {
        Ok(None)
    }

    fn get_segment(&self, location: Location) -> Result<Self::Segment, StorageError> {
        Err(StorageError::NoSuchStorage)
    }

    fn get_head(&self) -> Result<Location, StorageError> {
        Ok(Location::new(0, 0))
    }

    fn commit(&mut self, segment: Self::Segment) -> Result<(), StorageError> {
        Ok(())
    }

    fn write(&mut self, perspective: Self::Perspective) -> Result<Self::Segment, StorageError> {
        Err(StorageError::NoSuchStorage)
    }

    fn write_facts(
            &mut self,
            fact_perspective: Self::FactPerspective,
        ) -> Result<Self::FactIndex, StorageError> {
            Err(StorageError::NoSuchStorage)
    }
}

pub struct EmbeddedStorageProvider;

impl StorageProvider for EmbeddedStorageProvider {
    type Perspective = EmbeddedPerspective;
    type Segment = EmbeddedSegment;
    type Storage = EmbeddedStorage;

    fn new_perspective(&mut self, policy_id: PolicyId) -> Self::Perspective {
        EmbeddedPerspective
    }

    fn new_storage(
        &mut self,
        init: Self::Perspective,
    ) -> Result<(GraphId, &mut Self::Storage), StorageError> {
        Err(StorageError::NoSuchStorage)
    }

    fn get_storage(&mut self, graph: GraphId) -> Result<&mut Self::Storage, StorageError> {
        Err(StorageError::NoSuchStorage)
    }
}
