use crate::{ApplicationName, ClientId};
use bimap::BiMap;
use log::trace;
use std::sync::RwLock;
use up_rust::{UCode, UStatus};

type ClientAndAppName = BiMap<ClientId, ApplicationName>;

pub trait ApplicationRegistry {
    /// Get [ApplicationName] for a [ClientId]
    fn get_app_name_for_client_id(&self, client_id: ClientId) -> Option<ApplicationName>;

    /// Insert a [ClientId] and [ApplicationName]
    fn insert_client_and_app_name(
        &self,
        client_id: ClientId,
        app_name: ApplicationName,
    ) -> Result<(), UStatus>;

    /// Remove [ApplicationName] based on [ClientId]
    fn remove_app_name_for_client_id(&self, client_id: ClientId) -> Option<ApplicationName>;
}

pub struct InMemoryApplicationRegistry {
    client_and_app_name: RwLock<ClientAndAppName>,
}

impl InMemoryApplicationRegistry {
    pub fn new() -> Self {
        Self {
            client_and_app_name: RwLock::new(BiMap::new()),
        }
    }

    /// Get [ApplicationName] for a [ClientId]
    pub fn get_app_name_for_client_id(&self, client_id: ClientId) -> Option<ApplicationName> {
        let client_and_app_name = self.client_and_app_name.read().unwrap();

        trace!("client_and_app_name: {client_and_app_name:?}");

        let app_name = client_and_app_name.get_by_left(&client_id)?;

        Some(app_name.clone())
    }

    /// Insert a [ClientId] and [ApplicationName]
    pub fn insert_client_and_app_name(
        &self,
        client_id: ClientId,
        app_name: ApplicationName,
    ) -> Result<(), UStatus> {
        let mut client_and_app_name = self.client_and_app_name.write().unwrap();

        trace!("before insert_client_and_app_name: {client_and_app_name:?}");

        if let Err(existing) = client_and_app_name.insert_no_overwrite(client_id, app_name.clone())
        {
            trace!("failed insert_client_and_app_name: {client_and_app_name:?}");

            return Err(UStatus::fail_with_code(
                UCode::ALREADY_EXISTS,
                format!("Already exists that pair of client_id and app_name: {existing:?}"),
            ));
        }

        trace!("succeeded insert_client_and_app_name: {client_and_app_name:?}");

        Ok(())
    }

    /// Remove [ApplicationName] based on [ClientId]
    pub fn remove_app_name_for_client_id(&self, client_id: ClientId) -> Option<ApplicationName> {
        let mut client_and_app_name = self.client_and_app_name.write().unwrap();

        let (_client_id, app_name) = client_and_app_name.remove_by_left(&client_id)?;

        Some(app_name.clone())
    }
}
