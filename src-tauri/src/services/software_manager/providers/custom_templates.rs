pub struct CustomTemplate {
    pub id: &'static str,
    pub name_i18n: &'static str,
    pub executable: &'static str,
    pub args: &'static [&'static str],
    pub config_file_relative: Option<&'static str>,
}

pub fn builtin_templates() -> &'static [CustomTemplate] {
    &[
        CustomTemplate {
            id: "redis-server",
            name_i18n: "template.redisServer",
            executable: "redis-server.exe",
            args: &["{config_file}"],
            config_file_relative: Some("redis.conf"),
        },
        CustomTemplate {
            id: "nginx",
            name_i18n: "template.nginx",
            executable: "nginx.exe",
            args: &["-g", "daemon off;"],
            config_file_relative: Some("conf/nginx.conf"),
        },
        CustomTemplate {
            id: "generic",
            name_i18n: "template.generic",
            executable: "",
            args: &[],
            config_file_relative: None,
        },
    ]
}
