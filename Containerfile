FROM docker.io/library/debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN groupadd --gid 10001 locus \
    && useradd --uid 10001 --gid 10001 --home /data --shell /usr/sbin/nologin locus \
    && install -o 10001 -g 10001 -d /data
COPY --chmod=0755 locus-api /usr/local/bin/locus-api
WORKDIR /data
USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/locus-api"]
CMD ["serve", "--home", "/data"]
