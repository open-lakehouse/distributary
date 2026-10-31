//! The recipient handlers: recipients, their tokens and activation links.

use async_trait::async_trait;
use opensharing_admin::models::recipients::v1::{
    AuthenticationType, CreateRecipientRequest, DeleteRecipientRequest, GetRecipientRequest,
    ListRecipientsRequest, ListRecipientsResponse, Recipient, RecipientToken,
    RotateRecipientTokenRequest, UpdateRecipientRequest,
};
use opensharing_admin_api::RecipientHandler;

use super::{AdminAction, ProviderService, identifier};
use crate::Result;
use crate::recipients::{activation_url, issue_token};
use crate::store::{RecipientRecord, TokenRecord};
use opensharing_admin_api::{Error as AdminError, Result as AdminResult};

/// A token as shown to the provider; the activation link only until it is used.
pub fn listed_token(public_url: &str, token: &TokenRecord) -> RecipientToken {
    RecipientToken {
        id: token.id.clone(),
        created_at: token.created_at,
        activation_url: activation_url(public_url, token).unwrap_or_default(),
        expiration_time: token.expires_at.unwrap_or_default(),
        ..Default::default()
    }
}

impl<Cx> ProviderService<Cx> {
    /// A stored recipient with its tokens.
    async fn recipient(&self, record: RecipientRecord) -> Result<Recipient> {
        let tokens = self
            .store
            .tokens_for_recipient(&record.id)
            .await?
            .iter()
            .map(|t| listed_token(&self.public_url, t))
            .collect();
        Ok(Recipient {
            id: Some(record.id),
            name: record.name,
            authentication_type: AuthenticationType::TOKEN.into(),
            comment: record.comment,
            created_at: Some(record.created_at),
            tokens,
            ..Default::default()
        })
    }
}

#[async_trait]
impl<Cx: Send + Sync + 'static> RecipientHandler<Cx> for ProviderService<Cx> {
    async fn list_recipients(
        &self,
        _request: ListRecipientsRequest,
        context: Cx,
    ) -> AdminResult<ListRecipientsResponse> {
        self.authorize(AdminAction::ListRecipients, &context)
            .await?;
        let mut recipients = Vec::new();
        for record in self.store.list_recipients().await? {
            recipients.push(self.recipient(record).await?);
        }
        Ok(ListRecipientsResponse {
            recipients,
            ..Default::default()
        })
    }

    async fn create_recipient(
        &self,
        request: CreateRecipientRequest,
        context: Cx,
    ) -> AdminResult<Recipient> {
        self.authorize(
            AdminAction::CreateRecipient {
                recipient: &request.name,
            },
            &context,
        )
        .await?;
        match request.authentication_type.as_known() {
            Some(
                AuthenticationType::TOKEN | AuthenticationType::AUTHENTICATION_TYPE_UNSPECIFIED,
            ) => {}
            _ => {
                return Err(AdminError::invalid_argument(
                    "only TOKEN authentication is supported",
                ));
            }
        }
        identifier(&request.name)?;
        let record = self
            .store
            .create_recipient(&request.name, request.comment)
            .await?;
        issue_token(
            self.store.as_ref(),
            &self.secrets,
            &record,
            request.expiration_time,
        )
        .await?;
        Ok(self.recipient(record).await?)
    }

    async fn get_recipient(
        &self,
        request: GetRecipientRequest,
        context: Cx,
    ) -> AdminResult<Recipient> {
        self.authorize(
            AdminAction::ReadRecipient {
                recipient: &request.name,
            },
            &context,
        )
        .await?;
        let record = self.store.get_recipient(&request.name).await?;
        Ok(self.recipient(record).await?)
    }

    /// Update a recipient's comment. Renaming, changing the owner and setting
    /// properties are not supported.
    async fn update_recipient(
        &self,
        request: UpdateRecipientRequest,
        context: Cx,
    ) -> AdminResult<Recipient> {
        self.authorize(
            AdminAction::UpdateRecipient {
                recipient: &request.name,
            },
            &context,
        )
        .await?;
        if request.new_name.is_some() {
            return Err(AdminError::invalid_argument(
                "renaming recipients is not supported",
            ));
        }
        let current = self.store.get_recipient(&request.name).await?;
        let comment = request.comment.or(current.comment);
        let record = self.store.update_recipient(&request.name, comment).await?;
        Ok(self.recipient(record).await?)
    }

    async fn delete_recipient(
        &self,
        request: DeleteRecipientRequest,
        context: Cx,
    ) -> AdminResult<()> {
        self.authorize(
            AdminAction::DeleteRecipient {
                recipient: &request.name,
            },
            &context,
        )
        .await?;
        Ok(self.store.delete_recipient(&request.name).await?)
    }

    /// Replace the recipient's tokens with a fresh one. Existing tokens stop
    /// working immediately, which is always within the
    /// `existing_token_expire_in_seconds` the request may carry.
    async fn rotate_recipient_token(
        &self,
        request: RotateRecipientTokenRequest,
        context: Cx,
    ) -> AdminResult<Recipient> {
        self.authorize(
            AdminAction::RotateRecipientToken {
                recipient: &request.name,
            },
            &context,
        )
        .await?;
        let record = self.store.get_recipient(&request.name).await?;
        let expires_at = self
            .store
            .tokens_for_recipient(&record.id)
            .await?
            .first()
            .and_then(|t| t.expires_at);
        self.store.delete_tokens(&record.id).await?;
        issue_token(self.store.as_ref(), &self.secrets, &record, expires_at).await?;
        Ok(self.recipient(record).await?)
    }
}
