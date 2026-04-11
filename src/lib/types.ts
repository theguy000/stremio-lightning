// src/lib/types.ts

export interface ModMetadata {
  name: string;
  description: string;
  author: string;
  version: string;
  update_url?: string;
  source?: string;
  license?: string;
  homepage?: string;
  requirements?: string[];
}

export interface InstalledMod {
  filename: string;
  mod_type: string;
  metadata: ModMetadata | null;
}

export interface RegistryEntry {
  name: string;
  author: string;
  description?: string;
  version: string;
  repo: string;
  download: string;
  preview?: string;
}

export interface Registry {
  plugins: RegistryEntry[];
  themes: RegistryEntry[];
}

export interface UpdateInfo {
  has_update: boolean;
  installed_version?: string;
  new_version?: string;
  registry_version?: string;
  update_url?: string;
}

export interface SettingSchema {
  key: string;
  label: string;
  type: 'toggle' | 'input' | 'select';
  description?: string;
  default?: unknown;
  defaultValue?: unknown;
  options?: { label: string; value: string }[];
}
