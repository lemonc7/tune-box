FROM golang:1.26-alpine AS builder

WORKDIR /src

ENV GOPROXY=https://goproxy.cn,direct

COPY go.mod go.sum ./
RUN go mod download

COPY . .

# 禁用CGO
RUN CGO_ENABLED=0 GOOS=linux go build \
    -trimpath \
    -ldflags="-s -w" \
    -o /out/tune-go .


FROM alpine:3.22

RUN apk add --no-cache \
    ca-certificates \
    tzdata

COPY --from=builder /out/tune-go /app/tune-go

WORKDIR /app

EXPOSE 8080

ENTRYPOINT ["/app/tune-go"]
