-- Create the database for the pool. In the combined database, this DB is not
-- created via the `POSTGRES_DB` environment variable passed to the docker
-- container.
CREATE DATABASE pool_db;
\connect pool_db;
CREATE USER pool_user with password 'pool_password';
GRANT ALL ON SCHEMA public TO pool_user;
GRANT ALL ON DATABASE pool_db TO pool_user WITH GRANT OPTION;
