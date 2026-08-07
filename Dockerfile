FROM golang:1.26-alpine AS builder

WORKDIR /src

COPY go.mod go.sum ./
RUN go mod download

COPY . .

# 禁用CGO
RUN CGO_ENABLED=0 GOOS=linux go build \
    -trimpath \
    -ldflags="-s -w" \
    -o /out/tune-go \
    .

RUN apk add --no-cache ca-certificates


FROM scratch

COPY --from=builder /out/tune-go /app/tune-go
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

WORKDIR /app

EXPOSE 8080

ENTRYPOINT ["/app/tune-go"]
