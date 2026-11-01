-- Storage credentials and external locations moved to the storage-access
-- tables (`olai-uc-storage-access`, migrations 02xx, applied by the server with
-- its `unity` feature). Their rows are not carried over, since the new tables
-- hold UC `Credential`s rather than bare registered credentials: register the
-- credentials and external locations again through the admin API.

DROP TABLE external_locations;
DROP TABLE credentials;
