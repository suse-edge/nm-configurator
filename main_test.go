package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"github.com/suse-edge/nm-configurator/pkg/config"
	"github.com/suse-edge/nm-configurator/pkg/configurator"
)

const (
	sourceDir  = "testdata"
	configFile = "host_config.yaml"
	destDir    = "testdata/out"
)

func setupDestDir(t *testing.T) func(t *testing.T) {
	require.NoError(t, os.MkdirAll(destDir, 0755))

	return func(t *testing.T) {
		assert.NoError(t, os.RemoveAll(destDir))
	}
}

func TestConfigurator(t *testing.T) {
	teardown := setupDestDir(t)
	defer teardown(t)

	conf, err := config.Load(sourceDir, configFile, destDir)
	require.Nil(t, err)

	networkInterfaces := map[string]string{
		"00:11:22:33:44:55": "eth0",
		"00:11:22:33:44:56": "eth0.202", // Defined as "eth0.101" in eth0.101.nmconnection
		"00:11:22:33:44:57": "eth1",
		//"00:11:22:33:44:58": "bond0", Excluded on purpose, "bond0.nmconnection" should still be copied
	}

	c := configurator.New(conf, networkInterfaces)
	require.NoError(t, c.Run())

	// Verify the content of the copied files.
	hostDir := filepath.Join(sourceDir, "node1.example.com")
	entries, err := os.ReadDir(hostDir)
	require.Nil(t, err)

	assert.Len(t, entries, 4)

	for _, entry := range entries {
		filename := entry.Name()
		input, err := os.ReadFile(filepath.Join(hostDir, filename))
		require.Nil(t, err)

		// Adjust the name and content for the "eth0.101"->"eth0.202" edge case.
		if filename == "eth0.101.nmconnection" {
			filename = "eth0.202.nmconnection"
			input = []byte(strings.ReplaceAll(string(input), "eth0.101", "eth0.202"))
		}

		output, err := os.ReadFile(filepath.Join(destDir, filename))
		require.Nil(t, err)

		assert.Equal(t, string(input), string(output))
	}
}
