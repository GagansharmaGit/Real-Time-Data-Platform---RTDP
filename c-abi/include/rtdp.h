#ifndef RTDP_H
#define RTDP_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

#define RTDP_MAGIC 0x5254
#define RTDP_VERSION_1 0x01
#define RTDP_FIXED_HEADER_SIZE 48

/* Error Codes (Appendix A / error-codes.md) */
#define RTDP_ERR_VERSION_MISMATCH         1
#define RTDP_ERR_MALFORMED_FRAME          2
#define RTDP_ERR_CHECKSUM_FAILED          3
#define RTDP_ERR_UNSUPPORTED_MESSAGE_TYPE 4
#define RTDP_ERR_UNSUPPORTED_EXTENSION    5
#define RTDP_ERR_REPLAY_UNAVAILABLE       6
#define RTDP_ERR_REPLAY_RANGE_INVALID     7
#define RTDP_ERR_STREAM_NOT_FOUND         8
#define RTDP_ERR_PUBLISHER_NOT_AUTHORIZED 9
#define RTDP_ERR_FRAME_TOO_LARGE          10
#define RTDP_ERR_RATE_LIMITED             11
#define RTDP_ERR_BACKPRESSURE             12

/* Message Types */
#define RTDP_MSG_DATA                0x0001
#define RTDP_MSG_HEARTBEAT           0x0002
#define RTDP_MSG_REPLAY_REQUEST      0x0003
#define RTDP_MSG_REPLAY_RESPONSE     0x0004
#define RTDP_MSG_REPLAY_UNAVAILABLE  0x0005
#define RTDP_MSG_ERROR               0x0008

typedef struct {
    uint16_t magic;
    uint8_t  version;
    uint8_t  flags;
    uint16_t message_type;
    uint16_t header_length;
    uint64_t stream_id;
    uint64_t sequence;
    uint64_t timestamp_ns;
    uint64_t publisher_id;
    uint32_t payload_length;
    uint32_t checksum;
} rtdp_header_t;

typedef struct rtdp_codec_t rtdp_codec_t;

rtdp_codec_t* rtdp_codec_create(size_t max_frame_size, bool validate_checksum);
void rtdp_codec_destroy(rtdp_codec_t* codec);

int32_t rtdp_decode(
    rtdp_codec_t* codec,
    uint8_t* buf,
    size_t len,
    rtdp_header_t* out_header,
    const uint8_t** out_payload,
    size_t* out_payload_len
);

int32_t rtdp_encode(
    rtdp_codec_t* codec,
    const rtdp_header_t* in_header,
    const uint8_t* payload,
    size_t payload_len,
    uint8_t* out_buf,
    size_t out_cap,
    size_t* out_len
);

#ifdef __cplusplus
}
#endif

#endif /* RTDP_H */
