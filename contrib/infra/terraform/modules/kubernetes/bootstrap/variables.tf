variable "tailscale_operator_version" {
  type = string
}

variable "cluster_name" {
  type        = string
  description = "Name of the Kubernetes cluster"
}

variable "argocd_version" {
  type        = string
  description = "ArgoCD Helm chart version"
}

variable "argocd_tailscale_hostname" {
  type        = string
  description = "Tailscale hostname for ArgoCD ingress"
}

variable "coredns_version" {
  type        = string
  description = "CoreDNS Helm chart version"
}

variable "external_clusters" {
  type = map(string)
  description = "ts.net domain names of other Tailscale operators"
}

variable "onepassword_connect_version" {
  type = string
  description = "OnePassword operator Helm chart version"
}

variable "onepassword_secret_token" {
  type = string
  sensitive = true
  description = "OnePassword operator secret token"
}

variable "onepassword_credentials" {
  type = string
  sensitive = true
  description = "OnePassword Connect credentials"
}