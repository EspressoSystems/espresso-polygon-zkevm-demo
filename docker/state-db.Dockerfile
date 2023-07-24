FROM postgres:15
ENV POSTGRES_USER=state_user
ENV POSTGRES_PASSWORD=state_password
ENV POSTGRES_DB=state_db
ADD ./zkevm-node/db/scripts/init_prover_db.sql /docker-entrypoint-initdb.d/1.sql
ADD ./zkevm-node-additions/init_pool_db.sql /docker-entrypoint-initdb.d/2.sql
