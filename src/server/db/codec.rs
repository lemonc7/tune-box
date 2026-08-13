use sqlx::{
    Decode, Sqlite, Type,
    sqlite::{SqliteTypeInfo, SqliteValueRef},
    types::Json,
};

use crate::shared::music::Metadata;

impl Type<Sqlite> for Metadata {
    fn type_info() -> SqliteTypeInfo {
        <Json<Metadata> as Type<Sqlite>>::type_info()
    }

    fn compatible(ty: &SqliteTypeInfo) -> bool {
        <Json<Metadata> as Type<Sqlite>>::compatible(ty)
    }
}

impl<'row> Decode<'row, Sqlite> for Metadata {
    fn decode(value: SqliteValueRef<'row>) -> Result<Self, sqlx::error::BoxDynError> {
        <Json<Metadata> as Decode<'row, Sqlite>>::decode(value).map(|metadata| metadata.0)
    }
}
