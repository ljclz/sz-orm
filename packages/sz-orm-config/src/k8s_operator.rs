//! v7.6.0 K8s Operator 定义 + Helm Chart 模板。
//!
//! 产出 K8s Operator CRD 定义和 Helm Chart 模板，覆盖自动化部署/扩缩容/故障恢复。
//! 仅产出定义文件，不内置 K8s 客户端，由运维侧 `kubectl apply` / `helm install` 执行。

/// K8s Operator 定义
#[derive(Debug, Clone)]
pub struct K8sOperatorSpec {
    pub crd_name: String,
    pub auto_deploy: bool,
    pub auto_scale: bool,
    pub auto_recover: bool,
    pub spec_yaml: String,
}

impl K8sOperatorSpec {
    /// 生成 SZ-ORM Operator 定义
    pub fn generate_sz_orm_operator() -> Self {
        let spec_yaml = Self::build_operator_yaml();
        Self {
            crd_name: "szorminstances.szorm.io".to_string(),
            auto_deploy: true,
            auto_scale: true,
            auto_recover: true,
            spec_yaml,
        }
    }

    fn build_operator_yaml() -> String {
        r#"apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: szorminstances.szorm.io
spec:
  group: szorm.io
  names:
    kind: SzOrmInstance
    plural: szorminstances
    singular: szorminstance
  scope: Namespaced
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                replicas:
                  type: integer
                  minimum: 1
                  default: 3
                database:
                  type: string
                poolSize:
                  type: integer
                  default: 10
                autoScale:
                  type: boolean
                  default: true
                autoRecover:
                  type: boolean
                  default: true
            status:
              type: object
              properties:
                readyReplicas:
                  type: integer
                phase:
                  type: string
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: sz-orm-operator
spec:
  replicas: 1
  selector:
    matchLabels:
      app: sz-orm-operator
  template:
    metadata:
      labels:
        app: sz-orm-operator
    spec:
      containers:
        - name: operator
          image: sz-orm/operator:7.6.0
          command: ["/operator"]
          env:
            - name: WATCH_NAMESPACE
              valueFrom:
                fieldRef:
                  fieldPath: metadata.namespace
"#.to_string()
    }

    /// 验证 Operator 定义完整性
    pub fn validate(&self) -> bool {
        !self.crd_name.is_empty()
            && self.spec_yaml.contains("apiVersion")
            && self.spec_yaml.contains("kind")
    }
}

/// Helm Chart 模板
#[derive(Debug, Clone)]
pub struct HelmChartTemplate {
    pub chart_name: String,
    pub chart_version: String,
    pub values_yaml: String,
    pub templates: Vec<HelmTemplateFile>,
}

/// Helm 模板文件
#[derive(Debug, Clone)]
pub struct HelmTemplateFile {
    pub name: String,
    pub content: String,
}

impl HelmChartTemplate {
    /// 生成 SZ-ORM Helm Chart
    pub fn generate_sz_orm_chart() -> Self {
        let values_yaml = r#"replicaCount: 3
image:
  repository: sz-orm/server
  tag: "7.6.0"
  pullPolicy: IfNotPresent
database:
  type: postgres
  host: ""
  port: 5432
  name: sz_orm
pool:
  size: 10
  maxIdle: 5
autoScale:
  enabled: true
  minReplicas: 2
  maxReplicas: 10
  targetCPUUtilization: 70
autoRecover:
  enabled: true
  healthCheckInterval: 30
resources:
  limits:
    cpu: 1000m
    memory: 512Mi
  requests:
    cpu: 100m
    memory: 128Mi
"#
        .to_string();

        let templates = vec![
            HelmTemplateFile {
                name: "deployment.yaml".to_string(),
                content: r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ .Release.Name }}-sz-orm
spec:
  replicas: {{ .Values.replicaCount }}
  selector:
    matchLabels:
      app: {{ .Release.Name }}-sz-orm
  template:
    metadata:
      labels:
        app: {{ .Release.Name }}-sz-orm
    spec:
      containers:
        - name: sz-orm
          image: "{{ .Values.image.repository }}:{{ .Values.image.tag }}"
          resources:
            {{- toYaml .Values.resources | nindent 12 }}
"#
                .to_string(),
            },
            HelmTemplateFile {
                name: "hpa.yaml".to_string(),
                content: r#"{{- if .Values.autoScale.enabled }}
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: {{ .Release.Name }}-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: {{ .Release.Name }}-sz-orm
  minReplicas: {{ .Values.autoScale.minReplicas }}
  maxReplicas: {{ .Values.autoScale.maxReplicas }}
{{- end }}
"#
                .to_string(),
            },
        ];

        Self {
            chart_name: "sz-orm".to_string(),
            chart_version: "0.1.0".to_string(),
            values_yaml,
            templates,
        }
    }

    /// 验证 Chart 模板完整性
    pub fn validate(&self) -> bool {
        !self.chart_name.is_empty()
            && !self.values_yaml.is_empty()
            && !self.templates.is_empty()
            && self.templates.iter().all(|t| !t.content.is_empty())
    }

    /// 生成 Chart.yaml 内容
    pub fn chart_yaml(&self) -> String {
        format!(
            r#"apiVersion: v2
name: {}
description: SZ-ORM Helm Chart
type: application
version: {}
appVersion: "7.6.0"
"#,
            self.chart_name, self.chart_version
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn k8s_operator_spec_generate() {
        let spec = K8sOperatorSpec::generate_sz_orm_operator();
        assert_eq!(spec.crd_name, "szorminstances.szorm.io");
        assert!(spec.auto_deploy);
        assert!(spec.auto_scale);
        assert!(spec.auto_recover);
        assert!(spec.validate());
    }

    #[test]
    fn k8s_operator_yaml_contains_crd() {
        let spec = K8sOperatorSpec::generate_sz_orm_operator();
        assert!(spec.spec_yaml.contains("CustomResourceDefinition"));
        assert!(spec.spec_yaml.contains("SzOrmInstance"));
        assert!(spec.spec_yaml.contains("szorm.io"));
    }

    #[test]
    fn k8s_operator_yaml_contains_deployment() {
        let spec = K8sOperatorSpec::generate_sz_orm_operator();
        assert!(spec.spec_yaml.contains("Deployment"));
        assert!(spec.spec_yaml.contains("sz-orm-operator"));
    }

    #[test]
    fn helm_chart_generate() {
        let chart = HelmChartTemplate::generate_sz_orm_chart();
        assert_eq!(chart.chart_name, "sz-orm");
        assert!(!chart.values_yaml.is_empty());
        assert!(!chart.templates.is_empty());
        assert!(chart.validate());
    }

    #[test]
    fn helm_chart_yaml_contains_version() {
        let chart = HelmChartTemplate::generate_sz_orm_chart();
        let chart_yaml = chart.chart_yaml();
        assert!(chart_yaml.contains("7.6.0"));
        assert!(chart_yaml.contains("sz-orm"));
    }

    #[test]
    fn helm_chart_has_deployment_template() {
        let chart = HelmChartTemplate::generate_sz_orm_chart();
        assert!(chart.templates.iter().any(|t| t.name == "deployment.yaml"));
    }

    #[test]
    fn helm_chart_has_hpa_template() {
        let chart = HelmChartTemplate::generate_sz_orm_chart();
        assert!(chart.templates.iter().any(|t| t.name == "hpa.yaml"));
    }

    #[test]
    fn helm_values_contains_pool_config() {
        let chart = HelmChartTemplate::generate_sz_orm_chart();
        assert!(chart.values_yaml.contains("pool"));
        assert!(chart.values_yaml.contains("autoScale"));
        assert!(chart.values_yaml.contains("autoRecover"));
    }
}