use async_trait::async_trait;

use crate::error::DbError;
use crate::types::author::{Author, AuthorFilter, AuthorUpdate, NewAuthor};
use crate::types::collection::{Collection, CollectionUpdate, NewCollection};
use crate::types::edition::{Edition, EditionFilter, EditionUpdate, NewEdition};
use crate::types::file::{FileFilter, LibraryFile, LibraryFileUpdate, NewLibraryFile};
use crate::types::id::{
    AuthorId, CollectionId, EditionId, FileId, PublisherId, SeriesEntryId, SeriesId, TagId, WorkId,
};
use crate::types::publisher::{NewPublisher, Publisher, PublisherFilter, PublisherUpdate};
use crate::types::series::{
    NewSeries, NewSeriesEntry, Series, SeriesEntry, SeriesFilter, SeriesUpdate,
};
use crate::types::tag::{NewTag, Tag, TagUpdate};
use crate::types::work::{NewWork, Work, WorkFilter, WorkUpdate};
use crate::types::{Page, Pagination};

#[async_trait]
pub trait Repository: Send + Sync {
    // Works
    async fn create_work(&self, work: &NewWork) -> Result<Work, DbError>;
    async fn get_work(&self, id: WorkId) -> Result<Option<Work>, DbError>;
    async fn list_works(
        &self,
        filter: &WorkFilter,
        page: &Pagination,
    ) -> Result<Page<Work>, DbError>;
    async fn update_work(&self, id: WorkId, update: &WorkUpdate) -> Result<Work, DbError>;
    async fn delete_work(&self, id: WorkId) -> Result<(), DbError>;

    // Editions
    async fn create_edition(&self, edition: &NewEdition) -> Result<Edition, DbError>;
    async fn get_edition(&self, id: EditionId) -> Result<Option<Edition>, DbError>;
    async fn list_editions(
        &self,
        filter: &EditionFilter,
        page: &Pagination,
    ) -> Result<Page<Edition>, DbError>;
    async fn update_edition(
        &self,
        id: EditionId,
        update: &EditionUpdate,
    ) -> Result<Edition, DbError>;
    async fn delete_edition(&self, id: EditionId) -> Result<(), DbError>;

    // Authors
    async fn create_author(&self, author: &NewAuthor) -> Result<Author, DbError>;
    async fn get_author(&self, id: AuthorId) -> Result<Option<Author>, DbError>;
    async fn list_authors(
        &self,
        filter: &AuthorFilter,
        page: &Pagination,
    ) -> Result<Page<Author>, DbError>;
    async fn update_author(&self, id: AuthorId, update: &AuthorUpdate) -> Result<Author, DbError>;
    async fn delete_author(&self, id: AuthorId) -> Result<(), DbError>;

    // Work-Author associations
    async fn link_work_author(
        &self,
        work_id: WorkId,
        author_id: AuthorId,
        role: crate::types::enums::AuthorRole,
    ) -> Result<(), DbError>;
    async fn unlink_work_author(&self, work_id: WorkId, author_id: AuthorId)
    -> Result<(), DbError>;

    // Series
    async fn create_series(&self, series: &NewSeries) -> Result<Series, DbError>;
    async fn get_series(&self, id: SeriesId) -> Result<Option<Series>, DbError>;
    async fn list_series(
        &self,
        filter: &SeriesFilter,
        page: &Pagination,
    ) -> Result<Page<Series>, DbError>;
    async fn update_series(&self, id: SeriesId, update: &SeriesUpdate) -> Result<Series, DbError>;
    async fn delete_series(&self, id: SeriesId) -> Result<(), DbError>;

    // Series Entries
    async fn create_series_entry(&self, entry: &NewSeriesEntry) -> Result<SeriesEntry, DbError>;
    async fn delete_series_entry(&self, id: SeriesEntryId) -> Result<(), DbError>;
    async fn list_series_entries(&self, series_id: SeriesId) -> Result<Vec<SeriesEntry>, DbError>;

    // Publishers
    async fn create_publisher(&self, publisher: &NewPublisher) -> Result<Publisher, DbError>;
    async fn get_publisher(&self, id: PublisherId) -> Result<Option<Publisher>, DbError>;
    async fn list_publishers(
        &self,
        filter: &PublisherFilter,
        page: &Pagination,
    ) -> Result<Page<Publisher>, DbError>;
    async fn update_publisher(
        &self,
        id: PublisherId,
        update: &PublisherUpdate,
    ) -> Result<Publisher, DbError>;
    async fn delete_publisher(&self, id: PublisherId) -> Result<(), DbError>;

    // Collections
    async fn create_collection(&self, collection: &NewCollection) -> Result<Collection, DbError>;
    async fn get_collection(&self, id: CollectionId) -> Result<Option<Collection>, DbError>;
    async fn list_collections(&self, page: &Pagination) -> Result<Page<Collection>, DbError>;
    async fn update_collection(
        &self,
        id: CollectionId,
        update: &CollectionUpdate,
    ) -> Result<Collection, DbError>;
    async fn delete_collection(&self, id: CollectionId) -> Result<(), DbError>;

    // Collection-Work associations
    async fn add_work_to_collection(
        &self,
        collection_id: CollectionId,
        work_id: WorkId,
    ) -> Result<(), DbError>;
    async fn remove_work_from_collection(
        &self,
        collection_id: CollectionId,
        work_id: WorkId,
    ) -> Result<(), DbError>;

    // Tags
    async fn create_tag(&self, tag: &NewTag) -> Result<Tag, DbError>;
    async fn get_tag(&self, id: TagId) -> Result<Option<Tag>, DbError>;
    async fn list_tags(&self) -> Result<Vec<Tag>, DbError>;
    async fn update_tag(&self, id: TagId, update: &TagUpdate) -> Result<Tag, DbError>;
    async fn delete_tag(&self, id: TagId) -> Result<(), DbError>;

    // Work-Tag associations
    async fn tag_work(&self, work_id: WorkId, tag_id: TagId) -> Result<(), DbError>;
    async fn untag_work(&self, work_id: WorkId, tag_id: TagId) -> Result<(), DbError>;

    // Files
    async fn create_file(&self, file: &NewLibraryFile) -> Result<LibraryFile, DbError>;
    async fn get_file(&self, id: FileId) -> Result<Option<LibraryFile>, DbError>;
    async fn list_files(
        &self,
        filter: &FileFilter,
        page: &Pagination,
    ) -> Result<Page<LibraryFile>, DbError>;
    async fn update_file(
        &self,
        id: FileId,
        update: &LibraryFileUpdate,
    ) -> Result<LibraryFile, DbError>;
    async fn delete_file(&self, id: FileId) -> Result<(), DbError>;
    async fn find_file_by_hash(&self, sha256: &str) -> Result<Option<LibraryFile>, DbError>;
}
