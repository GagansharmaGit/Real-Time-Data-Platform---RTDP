#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include "../include/rtdp.h"

int main() {
    printf("============================================================\n");
    printf(" RTDP/1 C ABI Interoperability Proof Test\n");
    printf("============================================================\n");

    rtdp_codec_t* codec = rtdp_codec_create(1472, true);
    assert(codec != NULL);

    /* 1. Test encoding a DATA frame in C */
    const char* sample_payload = "HELLO FROM C ABI INTEROP";
    size_t payload_len = strlen(sample_payload);

    rtdp_header_t in_hdr;
    memset(&in_hdr, 0, sizeof(in_hdr));
    in_hdr.magic = RTDP_MAGIC;
    in_hdr.version = RTDP_VERSION_1;
    in_hdr.message_type = RTDP_MSG_DATA;
    in_hdr.header_length = RTDP_FIXED_HEADER_SIZE;
    in_hdr.stream_id = 777;
    in_hdr.sequence = 1;
    in_hdr.timestamp_ns = 1700000000000000000ULL;
    in_hdr.publisher_id = 999;
    in_hdr.payload_length = (uint32_t)payload_len;

    uint8_t wire_buf[1472];
    size_t encoded_len = 0;
    int32_t enc_rc = rtdp_encode(
        codec,
        &in_hdr,
        (const uint8_t*)sample_payload,
        payload_len,
        wire_buf,
        sizeof(wire_buf),
        &encoded_len
    );

    assert(enc_rc == 0);
    assert(encoded_len == RTDP_FIXED_HEADER_SIZE + payload_len);
    printf("1. C ABI Encoding: SUCCESS (%zu bytes encoded, CRC-32C calculated)\n", encoded_len);

    /* 2. Test decoding the encoded buffer in C */
    rtdp_header_t out_hdr;
    const uint8_t* out_payload = NULL;
    size_t out_payload_len = 0;

    int32_t dec_rc = rtdp_decode(
        codec,
        wire_buf,
        encoded_len,
        &out_hdr,
        &out_payload,
        &out_payload_len
    );

    assert(dec_rc == 0);
    assert(out_hdr.magic == RTDP_MAGIC);
    assert(out_hdr.version == RTDP_VERSION_1);
    assert(out_hdr.message_type == RTDP_MSG_DATA);
    assert(out_hdr.stream_id == 777);
    assert(out_hdr.sequence == 1);
    assert(out_hdr.publisher_id == 999);
    assert(out_payload_len == payload_len);
    assert(memcmp(out_payload, sample_payload, payload_len) == 0);
    printf("2. C ABI Decoding: SUCCESS (Payload and header validated)\n");

    /* 3. Test corruption detection in C */
    wire_buf[encoded_len - 1] ^= 0x55; /* Corrupt last payload byte */
    int32_t corrupt_rc = rtdp_decode(
        codec,
        wire_buf,
        encoded_len,
        &out_hdr,
        &out_payload,
        &out_payload_len
    );
    assert(corrupt_rc == -RTDP_ERR_CHECKSUM_FAILED);
    printf("3. C ABI Corruption Rejection: SUCCESS (Returned -RTDP_ERR_CHECKSUM_FAILED: %d)\n", corrupt_rc);

    rtdp_codec_destroy(codec);
    printf("------------------------------------------------------------\n");
    printf("C ABI Interoperability Proof: ALL TESTS PASSED!\n");
    printf("------------------------------------------------------------\n");
    return 0;
}
