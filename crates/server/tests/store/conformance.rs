//! The behavior every [`SharingStore`] shares, run against each backend
//! (`tests/{memory,sqlite,postgres}.rs` include this file).

use opensharing_server::Error;
use opensharing_server::store::{
    ExternalLocationRecord, SealedCredential, ShareObject, ShareObjectKind, SharingStore,
    TokenRecord,
};

/// Exercise every store operation against an empty `store`.
pub async fn round_trips(store: &dyn SharingStore) {
    // Shares and their objects.
    let share = store
        .create_share("conformance_share", Some("c".into()))
        .await
        .unwrap();
    assert!(matches!(
        store.create_share("conformance_share", None).await,
        Err(Error::InvalidArgument(_))
    ));
    let object = ShareObject {
        start_version: Some(3),
        ..ShareObject::new(ShareObjectKind::Table, "main.sales.orders")
    };
    let updated = store
        .update_share("conformance_share", vec![object.clone()], None)
        .await
        .unwrap();
    assert_eq!(updated.id, share.id);
    assert_eq!(
        store.get_share("conformance_share").await.unwrap().objects,
        vec![object]
    );

    // Recipients, tokens and grants.
    let recipient = store
        .create_recipient("conformance_recipient", None)
        .await
        .unwrap();
    let updated = store
        .update_recipient("conformance_recipient", Some("updated".to_string()))
        .await
        .unwrap();
    assert_eq!(updated.comment.as_deref(), Some("updated"));
    assert_eq!(updated.id, recipient.id);
    store
        .insert_token(TokenRecord {
            id: "t1".into(),
            recipient_id: recipient.id.clone(),
            token_hash: vec![1, 2, 3],
            sealed_token: Some(vec![9]),
            activation_code: Some("code".into()),
            expires_at: None,
            created_at: 1,
        })
        .await
        .unwrap();
    let (token, found) = store.token_by_hash(&[1, 2, 3]).await.unwrap();
    assert_eq!(found.name, "conformance_recipient");
    assert_eq!(
        store.token_by_activation_code("code").await.unwrap().id,
        token.id
    );
    assert_eq!(store.sealed_tokens().await.unwrap().len(), 1);
    store.mark_activated("t1").await.unwrap();
    assert!(matches!(
        store.token_by_activation_code("code").await,
        Err(Error::NotFound)
    ));
    assert!(store.sealed_tokens().await.unwrap().is_empty());

    assert!(
        !store
            .is_granted("conformance_share", &recipient.id)
            .await
            .unwrap()
    );
    store
        .grant("conformance_share", "conformance_recipient")
        .await
        .unwrap();
    store
        .grant("conformance_share", "conformance_recipient")
        .await
        .unwrap();
    assert!(
        store
            .is_granted("conformance_share", &recipient.id)
            .await
            .unwrap()
    );
    assert_eq!(
        store.recipients_of("conformance_share").await.unwrap(),
        ["conformance_recipient"]
    );
    assert_eq!(
        store.shares_of(&recipient.id).await.unwrap()[0].name,
        "conformance_share"
    );
    assert!(matches!(
        store.grant("conformance_share", "nobody").await,
        Err(Error::NotFound)
    ));
    store
        .revoke("conformance_share", "conformance_recipient")
        .await
        .unwrap();
    assert!(
        !store
            .is_granted("conformance_share", &recipient.id)
            .await
            .unwrap()
    );

    // Sealed credentials and external locations.
    store
        .put_credential(SealedCredential {
            name: "cred".into(),
            sealed: vec![7],
        })
        .await
        .unwrap();
    store
        .put_external_location(ExternalLocationRecord {
            name: "loc".into(),
            url: "s3://bucket/prefix".into(),
            credential_name: "cred".into(),
        })
        .await
        .unwrap();
    assert_eq!(store.list_external_locations().await.unwrap().len(), 1);
    assert_eq!(store.get_credential("cred").await.unwrap().sealed, vec![7]);

    // Deleting a recipient cascades to its tokens and grants.
    store
        .grant("conformance_share", "conformance_recipient")
        .await
        .unwrap();
    store
        .delete_recipient("conformance_recipient")
        .await
        .unwrap();
    assert!(matches!(
        store.token_by_hash(&[1, 2, 3]).await,
        Err(Error::NotFound)
    ));
    assert!(
        store
            .recipients_of("conformance_share")
            .await
            .unwrap()
            .is_empty()
    );
    store.delete_share("conformance_share").await.unwrap();
    assert!(matches!(
        store.get_share("conformance_share").await,
        Err(Error::NotFound)
    ));
}
