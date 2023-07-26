FROM postgres:15
ENV POSTGRES_USER=postgres
ENV POSTGRES_PASSWORD=root_password
ENV POSTGRES_DB=unused
ADD ./zkevm-node/db/scripts/init_prover_db.sql /docker-entrypoint-initdb.d/1.sql
ADD ./zkevm-node-additions/init_pool_db.sql /docker-entrypoint-initdb.d/2.sql
ADD ./zkevm-node-additions/init_state_db.sql /docker-entrypoint-initdb.d/3.sql
