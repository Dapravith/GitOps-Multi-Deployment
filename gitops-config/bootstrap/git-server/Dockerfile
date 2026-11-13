# Minimal READ-ONLY Git smart-HTTP server for the kind lab (Argo CD's source
# until the config repo moves to GitHub). lighttpd runs `git http-backend` as
# CGI; pushes over HTTP are rejected (no auth => receive-pack disabled).
FROM alpine:3.24.4@sha256:f59abe656b692d901e184be3445e084786602108599d2a51d229c0f5a48c71bf

# hadolint ignore=DL3018
RUN apk add --no-cache git git-daemon lighttpd \
 && git config --system safe.directory '*' \
 && git config --system http.receivepack false \
 && git config --system uploadpack.allowReachableSHA1InWant true

COPY lighttpd.conf /etc/lighttpd/lighttpd.conf

# Unprivileged port, unprivileged user (alpine `lighttpd` = 100:101).
USER 100:101
EXPOSE 8080
ENTRYPOINT ["lighttpd", "-D", "-f", "/etc/lighttpd/lighttpd.conf"]
