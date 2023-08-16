package config

import (
	"os"
	"path/filepath"

	"gopkg.in/yaml.v3"
)

type Config struct {
	// Configuration directory storing the preconfigured *.nmconnection files per host.
	SourceDir string
	// Destination directory to store the final *.nmconnection files for NetworkManager.
	// Default "/etc/NetworkManager/system-connections".
	DestinationDir string
	Hosts          []*Host `yaml:"host_config"`
}

type Host struct {
	Name       string       `yaml:"hostname"`
	Interfaces []*Interface `yaml:"interfaces"`
}

type Interface struct {
	LogicalName string `yaml:"logical_name"`
	MACAddress  string `yaml:"mac_address"`
}

func Load(sourceDir, configFilename, destinationDir string) (*Config, error) {
	configFile := filepath.Join(sourceDir, configFilename)
	file, err := os.ReadFile(configFile)
	if err != nil {
		return nil, err
	}

	var c Config
	if err = yaml.Unmarshal(file, &c); err != nil {
		return nil, err
	}

	c.SourceDir = sourceDir
	c.DestinationDir = destinationDir

	return &c, nil
}
