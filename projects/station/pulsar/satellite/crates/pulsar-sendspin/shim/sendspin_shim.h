#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ss_client ss_client;
typedef struct ss_blob ss_blob;

typedef struct ss_str {
  const char *ptr;
  size_t len;
} ss_str;

enum { SS_CODEC_FLAC = 0, SS_CODEC_OPUS = 1, SS_CODEC_PCM = 2, SS_CODEC_UNSUPPORTED = 3 };
enum { SS_VIS_BEAT = 0, SS_VIS_LOUDNESS = 1, SS_VIS_F_PEAK = 2, SS_VIS_SPECTRUM = 3, SS_VIS_PEAK = 4 };
enum { SS_SCALE_MEL = 0, SS_SCALE_LOG = 1, SS_SCALE_LIN = 2 };
enum { SS_CMD_PLAY = 0, SS_CMD_PAUSE = 1, SS_CMD_STOP = 2, SS_CMD_NEXT = 3, SS_CMD_PREVIOUS = 4 };
enum { SS_PLAYBACK_ABSENT = -1, SS_PLAYBACK_PLAYING = 0, SS_PLAYBACK_STOPPED = 1 };
enum { SS_LOG_NONE = 0, SS_LOG_ERROR = 1, SS_LOG_WARN = 2, SS_LOG_INFO = 3, SS_LOG_DEBUG = 4 };

typedef struct ss_format {
  uint32_t sample_rate;
  uint8_t codec;
  uint8_t channels;
  uint8_t bit_depth;
} ss_format;

typedef struct ss_config {
  ss_str name;
  ss_str product_name;
  ss_str manufacturer;
  ss_str software_version;
  ss_str mac;
  ss_str pairing_psk_location;
  const ss_format *formats;
  size_t n_formats;
  const uint8_t *vis_types;
  size_t n_vis_types;
  size_t vis_buffer_capacity;
  int32_t vis_display_offset_ms;
  uint16_t port;
  uint16_t vis_rate_max;
  uint16_t vis_f_min;
  uint16_t vis_f_max;
  uint8_t vis_bins;
  uint8_t vis_scale;
  bool pairing_code_display_digits;
  bool unpaired_access;
} ss_config;

typedef struct ss_callbacks {
  void (*group_update)(void *user, int8_t playback_state, const char *name, size_t name_len);
  void (*time_sync)(void *user, float error_us);
  void (*pairing_started)(void *user, const char *server_id, size_t len);
  void (*pairing_succeeded)(void *user, const char *server_id, size_t len);
  void (*pairing_failed)(void *user, const char *server_id, size_t len, uint8_t reason);
  void (*trust_changed)(void *user, bool paired);
  void (*display_pairing_code)(void *user, const char *code, size_t len);
  void (*clear_pairing_code)(void *user);
  void (*open_pairing_window)(void *user);
  void (*close_pairing_window)(void *user);
  void (*stream_start)(void *user, bool complete, uint8_t codec, uint32_t rate, uint8_t channels, uint8_t bits);
  void (*stream_end)(void *user);
  void (*volume_changed)(void *user, uint8_t volume);
  void (*mute_changed)(void *user, bool muted);
  void (*output_delay_changed)(void *user, uint16_t delay_ms);
  void (*controller_state)(void *user, size_t n_commands, uint8_t volume, bool muted);
  void (*metadata)(void *user, const char *artist, size_t artist_len, const char *title, size_t title_len);
  void (*visualizer_stream_start)(void *user, size_t n_types, uint16_t rate_max);
  void (*visualizer_stream_end)(void *user);
  bool (*load_blob)(void *user, const char *key, size_t key_len, ss_blob *out);
  bool (*save_blob)(void *user, const char *key, size_t key_len, const uint8_t *data, size_t len);
  size_t (*audio_write)(void *user, const uint8_t *data, size_t len, uint32_t timeout_ms, uint32_t *played_frames,
                        int64_t *finish_us);
  void (*loudness)(void *user, uint16_t loudness);
  void (*beat)(void *user);
  void (*spectrum)(void *user, const uint16_t *bins, size_t n);
} ss_callbacks;

typedef void (*ss_put)(void *dst, const char *s, size_t len);

void ss_set_log_level(uint8_t level);
int64_t ss_now_us(void);

ss_client *ss_create(const ss_config *cfg, const ss_callbacks *cb, void *user);
void ss_destroy(ss_client *c);
int ss_start(ss_client *c);
bool ss_loop(ss_client *c);
bool ss_is_connected(const ss_client *c);
bool ss_server_info(const ss_client *c, ss_put put, void *name_dst, void *id_dst);
bool ss_client_id(const ss_client *c, ss_put put, void *dst);
bool ss_pairing_token(const ss_client *c, ss_put put, void *dst);
void ss_player_set_output_delay_adjustable(ss_client *c, bool adjustable);
uint8_t ss_player_volume(const ss_client *c);
bool ss_player_muted(const ss_client *c);
void ss_player_update_volume(ss_client *c, uint8_t volume);
void ss_player_update_muted(ss_client *c, bool muted);
bool ss_controller_send(ss_client *c, uint8_t command);
void ss_leave(ss_client *c);
void ss_disconnect_restart(ss_client *c);
void ss_confirm_pairing_window(ss_client *c);
bool ss_blob_set(ss_blob *blob, const uint8_t *data, size_t len);

#ifdef __cplusplus
}
#endif
