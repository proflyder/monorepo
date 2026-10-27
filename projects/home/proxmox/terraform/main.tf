terraform {
  required_version = "= 1.17.0"
  required_providers {
    proxmox = {
      source  = "bpg/proxmox"
      version = "= 0.118.1"
    }
  }
  # State directory is supplied with -chdir; never keep state in public source.
}

provider "proxmox" {
  # Credentials/endpoint via PROXMOX_VE_*; CA via SSL_CERT_FILE.
  insecure = false
}

variable "guest_ssh_public_key" {
  type = string
}

resource "proxmox_virtual_environment_vm" "haos" {
  node_name       = "pve"
  vm_id           = 100
  name            = "haos"
  description     = "Home Assistant OS; managed by Terraform"
  tags            = ["terraform", "home-assistant"]
  bios            = "ovmf"
  machine         = "q35"
  scsi_hardware   = "virtio-scsi-single"
  boot_order      = ["scsi0"]
  on_boot         = true
  started         = true
  stop_on_destroy = false
  protection      = true
  cpu {
    cores = 2
    type  = "host"
  }
  memory {
    dedicated = 3072
  }
  agent {
    enabled = true
    wait_for_ip { disabled = true }
  }
  efi_disk {
    datastore_id      = "local-lvm"
    type              = "4m"
    pre_enrolled_keys = false
  }
  disk {
    datastore_id = "local-lvm"
    import_from  = "local:import/haos_ova-18.4.qcow2"
    interface    = "scsi0"
    size         = 32
    discard      = "on"
    iothread     = true
    ssd          = true
  }
  network_device { bridge = "vmbr0" }
  operating_system { type = "l26" }
  startup {
    order      = "1"
    up_delay   = 30
    down_delay = 60
  }
  lifecycle { prevent_destroy = true }
}

resource "proxmox_virtual_environment_vm" "k3s" {
  node_name       = "pve"
  vm_id           = 101
  name            = "lab-k3s"
  description     = "Ubuntu 24.04 LTS / K3s; managed by Terraform"
  tags            = ["terraform", "kubernetes"]
  scsi_hardware   = "virtio-scsi-single"
  boot_order      = ["scsi0"]
  on_boot         = true
  started         = true
  stop_on_destroy = false
  protection      = true
  cpu {
    cores = 8
    type  = "host"
  }
  memory { dedicated = 9216 }
  agent {
    enabled = true
    wait_for_ip { disabled = true }
  }
  disk {
    datastore_id = "local-lvm"
    import_from  = "local:import/noble-server-cloudimg-amd64.qcow2"
    interface    = "scsi0"
    size         = 200
    discard      = "on"
    iothread     = true
    ssd          = true
  }
  initialization {
    datastore_id      = "local-lvm"
    user_data_file_id = "local:snippets/lab-k3s-cloud-init.yaml"
    ip_config {
      ipv4 { address = "dhcp" }
    }
    user_account {
      username = "labuser"
      keys     = [trimspace(var.guest_ssh_public_key)]
    }
  }
  network_device { bridge = "vmbr0" }
  operating_system { type = "l26" }
  serial_device {}
  startup {
    order      = "2"
    up_delay   = 30
    down_delay = 90
  }
  lifecycle { prevent_destroy = true }
}
