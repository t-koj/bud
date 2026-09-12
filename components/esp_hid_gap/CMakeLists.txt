idf_component_register(SRCS "esp_hid_gap.c"
                       INCLUDE_DIRS "."
                       REQUIRES esp_hid
                       PRIV_REQUIRES nvs_flash)
