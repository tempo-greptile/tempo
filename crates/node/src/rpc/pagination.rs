use crate::rpc::dex::OrdersSort;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginationParams<Filters> {
    /// Cursor for pagination.
    ///
    /// The cursor format depends on the endpoint:
    /// - `dex_getOrders`: Order ID (u128 encoded as string)
    /// - `dex_getOrderbooks`: Book Key (B256 encoded as hex string)
    ///
    /// Defaults to first entry based on the sort and filter configuration.
    /// Use the `nextCursor` in response to get the next set of results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,

    /// Determines which items should be yielded in the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<Filters>,

    /// Maximum number of orders to return.
    ///
    /// Defaults to 10.
    /// Maximum is 100.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,

    /// Determines the order of the items yielded in the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<OrdersSort>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterRange<T> {
    pub min: Option<T>,
    pub max: Option<T>,
}

/// A wrapper type with an optional cursor to the next page in a paginated response.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithCursor<T> {
    #[serde(flatten)]
    inner: T,
    next_cursor: Option<String>,
}

impl<T> WithCursor<T> {
    pub fn new(inner: T, next_cursor: Option<String>) -> Self {
        Self { inner, next_cursor }
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn next_cursor(&self) -> Option<&String> {
        self.next_cursor.as_ref()
    }
}

impl<T: PartialOrd> FilterRange<T> {
    /// Checks if a value is within this range (inclusive)
    pub fn in_range(&self, value: T) -> bool {
        if self.min.as_ref().is_some_and(|min| &value < min) {
            return false;
        }

        if self.max.as_ref().is_some_and(|max| &value > max) {
            return false;
        }

        true
    }
}
