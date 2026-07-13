use crate::models::software::CustomHealthSpec;

pub struct CustomTemplate {
    pub id: &'static str,
    pub name_i18n: &'static str,
    pub executable: &'static str,
    pub args: &'static [&'static str],
    pub config_file_relative: Option<&'static str>,
}

impl CustomTemplate {
    /// 返回该模板的默认健康检查 spec
    pub fn default_health_spec(&self) -> CustomHealthSpec {
        match self.id {
            "redis-server" => CustomHealthSpec::Tcp { port: 6379 },
            "nginx" => CustomHealthSpec::Http {
                url: "http://127.0.0.1/".to_string(),
                expected_status: 200,
            },
            _ => CustomHealthSpec::None,
        }
    }
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

pub fn find_template(id: &str) -> Option<&'static CustomTemplate> {
    builtin_templates().iter().find(|t| t.id == id)
}
