#include <stdint.h>
typedef int64_t   UnixMs;

typedef enum Status { Status_Active, Status_Disabled } Status;

typedef struct User {
    int64_t id;
    UnixMs  created_at;
    Status  status;
    char*   display_name;
} User;

typedef struct UserStore {
    int32_t (* get)(void* self, int64_t id, User* out);
    int32_t (* list)(void* self, User* out, unsigned long cap);
} UserStore;
