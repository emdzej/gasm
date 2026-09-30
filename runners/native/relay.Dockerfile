# gasm-relay container image: packages the release binaries (built natively per
# arch by release.yml), so the multi-arch build is a copy, not an emulated compile.
#   docker run -p 9000:9000 ghcr.io/emdzej/gasm-relay
#   docker run -p 9443:9443 -v /etc/letsencrypt:/certs:ro ghcr.io/emdzej/gasm-relay \
#     0.0.0.0:9443 --tls-cert /certs/live/relay.example.com/fullchain.pem --tls-key /certs/live/relay.example.com/privkey.pem
FROM debian:trixie-slim
ARG TARGETARCH
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY relay-${TARGETARCH} /usr/local/bin/gasm-relay
USER 65534:65534
EXPOSE 9000
ENTRYPOINT ["/usr/local/bin/gasm-relay"]
CMD ["0.0.0.0:9000"]
