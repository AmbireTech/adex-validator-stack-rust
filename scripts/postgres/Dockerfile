FROM postgres:latest
COPY ./create-multiple-postgres-db.sh /docker-entrypoint-initdb.d


CMD ["docker-entrypoint.sh", "postgres"]