-- Create the database for the state. In the combined database, this DB is not
-- created via the `POSTGRES_DB` environment variable passed to the docker
-- container.
CREATE DATABASE state_db;
\connect state_db;
CREATE USER state_user with password 'state_password';
GRANT ALL ON SCHEMA public TO state_user;
GRANT ALL ON DATABASE state_db TO state_user WITH GRANT OPTION;
