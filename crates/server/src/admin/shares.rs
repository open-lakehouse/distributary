//! The share and share-permission handlers.

use std::collections::HashSet;

use async_trait::async_trait;
use opensharing_admin::models::shares::v1::{
    Action, CreateShareRequest, DataObject, DataObjectType, DeleteShareRequest,
    GetPermissionsRequest, GetPermissionsResponse, GetShareRequest, ListSharesRequest,
    ListSharesResponse, PrivilegeAssignment, Share, UpdatePermissionsRequest,
    UpdatePermissionsResponse, UpdateShareRequest,
};
use opensharing_api::admin::ShareHandler;

use super::{AdminAction, ProviderService, identifier};
use crate::catalog::shared_as;
use crate::source::split_name;
use crate::store::{ShareObject, ShareRecord, now_millis};
use crate::{Error, Result};

/// The only privilege a share grants.
const SELECT: &str = "SELECT";

/// A `DataObject` as the store's [`ShareObject`]. Both use the same field names,
/// so the conversion goes through JSON and keeps every field.
fn share_object(object: &DataObject) -> Result<ShareObject> {
    serde_json::from_value(serde_json::to_value(object)?).map_err(|_| {
        Error::invalid_argument(format!(
            "data object '{}' needs a data_object_type of TABLE, SCHEMA, VOLUME or AGENT_SKILL",
            object.name
        ))
    })
}

/// A stored [`ShareObject`] as a `DataObject`.
fn data_object(object: &ShareObject) -> Result<DataObject> {
    Ok(serde_json::from_value(serde_json::to_value(object)?)?)
}

fn share(record: ShareRecord) -> Result<Share> {
    Ok(Share {
        id: Some(record.id),
        name: record.name,
        objects: record
            .objects
            .iter()
            .map(data_object)
            .collect::<Result<_>>()?,
        comment: record.comment,
        created_at: Some(record.created_at),
        updated_at: Some(record.updated_at),
        ..Default::default()
    })
}

/// Check a data object is well formed for its type.
fn validate_object(object: &DataObject) -> Result<()> {
    match object.data_object_type.as_known() {
        Some(DataObjectType::TABLE | DataObjectType::VOLUME | DataObjectType::AGENT_SKILL) => {
            split_name::<3>(&object.name)?;
            shared_as(&share_object(object)?)?;
        }
        Some(DataObjectType::SCHEMA) => {
            split_name::<2>(&object.name)?;
            if let Some(shared) = &object.shared_as {
                identifier(shared)?;
            }
        }
        _ => {
            return Err(Error::invalid_argument(format!(
                "data object '{}' needs a data_object_type of TABLE, SCHEMA, VOLUME or AGENT_SKILL",
                object.name
            )));
        }
    }
    Ok(())
}

fn assignments(recipients: Vec<String>) -> Vec<PrivilegeAssignment> {
    recipients
        .into_iter()
        .map(|principal| PrivilegeAssignment {
            principal,
            privileges: vec![SELECT.to_string()],
            ..Default::default()
        })
        .collect()
}

#[async_trait]
impl<Cx: Send + Sync + 'static> ShareHandler<Cx> for ProviderService<Cx> {
    async fn list_shares(
        &self,
        _request: ListSharesRequest,
        context: Cx,
    ) -> Result<ListSharesResponse> {
        self.authorize(AdminAction::ListShares, &context).await?;
        Ok(ListSharesResponse {
            shares: self
                .store
                .list_shares()
                .await?
                .into_iter()
                .map(share)
                .collect::<Result<_>>()?,
            ..Default::default()
        })
    }

    async fn create_share(&self, request: CreateShareRequest, context: Cx) -> Result<Share> {
        self.authorize(
            AdminAction::CreateShare {
                share: &request.name,
            },
            &context,
        )
        .await?;
        identifier(&request.name)?;
        share(
            self.store
                .create_share(&request.name, request.comment)
                .await?,
        )
    }

    async fn get_share(&self, request: GetShareRequest, context: Cx) -> Result<Share> {
        self.authorize(
            AdminAction::ReadShare {
                share: &request.name,
            },
            &context,
        )
        .await?;
        share(self.store.get_share(&request.name).await?)
    }

    async fn update_share(&self, request: UpdateShareRequest, context: Cx) -> Result<Share> {
        self.authorize(
            AdminAction::UpdateShare {
                share: &request.name,
            },
            &context,
        )
        .await?;
        if request.new_name.is_some() {
            return Err(Error::invalid_argument("renaming shares is not supported"));
        }
        let current = self.store.get_share(&request.name).await?;
        let mut objects = current
            .objects
            .iter()
            .map(data_object)
            .collect::<Result<Vec<_>>>()?;
        for update in request.updates {
            let object = update
                .data_object
                .as_option()
                .cloned()
                .ok_or_else(|| Error::invalid_argument("a share update needs a data_object"))?;
            let position = objects.iter().position(|o| o.name == object.name);
            match (update.action.as_known(), position) {
                (Some(Action::ADD), None) => {
                    validate_object(&object)?;
                    objects.push(DataObject {
                        added_at: Some(now_millis()),
                        ..object
                    });
                }
                (Some(Action::ADD), Some(_)) => {
                    return Err(Error::invalid_argument(format!(
                        "'{}' is already in the share",
                        object.name
                    )));
                }
                (Some(Action::UPDATE), Some(i)) => {
                    validate_object(&object)?;
                    objects[i] = DataObject {
                        added_at: objects[i].added_at,
                        ..object
                    };
                }
                (Some(Action::REMOVE), Some(i)) => {
                    objects.remove(i);
                }
                (Some(Action::UPDATE | Action::REMOVE), None) => {
                    return Err(Error::invalid_argument(format!(
                        "'{}' is not in the share",
                        object.name
                    )));
                }
                _ => return Err(Error::invalid_argument("unknown share update action")),
            }
        }
        // Two objects may not be exposed under the same shared name.
        let mut exposed = HashSet::new();
        for object in objects
            .iter()
            .filter(|o| o.data_object_type.as_known() != Some(DataObjectType::SCHEMA))
        {
            let key = (
                object.data_object_type.to_i32(),
                shared_as(&share_object(object)?)?,
            );
            if !exposed.insert(key) {
                return Err(Error::invalid_argument(format!(
                    "'{}' would be shared under a name already in use",
                    object.name
                )));
            }
        }
        let comment = request.comment.or(current.comment);
        let objects = objects
            .iter()
            .map(share_object)
            .collect::<Result<Vec<_>>>()?;
        share(
            self.store
                .update_share(&request.name, objects, comment)
                .await?,
        )
    }

    async fn delete_share(&self, request: DeleteShareRequest, context: Cx) -> Result<()> {
        self.authorize(
            AdminAction::DeleteShare {
                share: &request.name,
            },
            &context,
        )
        .await?;
        self.store.delete_share(&request.name).await
    }

    async fn get_permissions(
        &self,
        request: GetPermissionsRequest,
        context: Cx,
    ) -> Result<GetPermissionsResponse> {
        self.authorize(
            AdminAction::ReadPermissions {
                share: &request.name,
            },
            &context,
        )
        .await?;
        self.store.get_share(&request.name).await?;
        Ok(GetPermissionsResponse {
            privilege_assignments: assignments(self.store.recipients_of(&request.name).await?),
            ..Default::default()
        })
    }

    async fn update_permissions(
        &self,
        request: UpdatePermissionsRequest,
        context: Cx,
    ) -> Result<UpdatePermissionsResponse> {
        self.authorize(
            AdminAction::UpdatePermissions {
                share: &request.name,
            },
            &context,
        )
        .await?;
        for change in &request.changes {
            for privilege in change.add.iter().chain(&change.remove) {
                if !privilege.eq_ignore_ascii_case(SELECT) {
                    return Err(Error::invalid_argument(format!(
                        "unsupported privilege '{privilege}'; shares grant SELECT"
                    )));
                }
            }
        }
        for change in request.changes {
            if !change.add.is_empty() {
                self.store.grant(&request.name, &change.principal).await?;
            }
            if !change.remove.is_empty() {
                self.store.revoke(&request.name, &change.principal).await?;
            }
        }
        Ok(UpdatePermissionsResponse {
            privilege_assignments: assignments(self.store.recipients_of(&request.name).await?),
            ..Default::default()
        })
    }
}
