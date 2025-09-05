variable "cluster_name" {
  type        = string
  description = "Name of the Kubernetes cluster"
}

variable "tailscale_operator_version" {
  type = string
}

variable "coredns_version" {
  type        = string
  description = "CoreDNS Helm chart version"
}