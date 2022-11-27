---
title: Best Practices
---

# Best Practices

## Manage wdeployment IDs

With the hardshare client, you can freely create and destroy workspace
deployments. This process corresponds to creating or destroying a unique ID.
Here, "destroying a unique ID" means that the corresponding workspace deployment
is marked as permanently unavailable.

When some part of robot or the surrounding environment changes significantly,
the unique ID should be changed.  What is "significant" or not depends on the
context. For example, removing a LiDAR sensor is likely significant, but small
changes to overhead lighting might not be.

Ensuring that unique IDs correspond to a known setting is a best practice
because it facilitates automation. For example, automated tests can assume that,
if the same ID is referenced, then the testing fixture with real hardware is the
same (up to some tolerance).
