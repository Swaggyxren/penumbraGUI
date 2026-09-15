#!/usr/bin/env python3
#
# SPDX-FileCopyrightText: 2025 Shomy
# SPDX-License-Identifier: AGPL-3.0-or-later
#
from fastapi import FastAPI
from pydantic import BaseModel
from typing import List

app = FastAPI()

class LoginRequest(BaseModel):
    username: str
    password: str

class ApiSignData(BaseModel):
    rnd: str
    soc_id: str
    hrid: str
    raw: str

class ApiSignRequest(BaseModel):
    data: ApiSignData
    purpose: str
    pubk_mod: str

class CanSignRequest(BaseModel):
    pubk_mod: str


@app.get("/api/auth/me")
def auth_me():
    return {
        "is_admin": True,
        "permissions": ["brom_sla", "pl_sla", "meta_sla", "da_sla"]
    }

@app.post("/api/auth/refresh")
def auth_refresh():
    return {
        "access_token": "mock_access_token",
        "refresh_token": "mock_refresh_token",
        "expires_in": 3600
    }

@app.post("/api/auth/login")
def auth_login(req: LoginRequest):
    return {
        "access_token": "mock_access_token",
        "refresh_token": "mock_refresh_token",
        "expires_in": 3600
    }


@app.post("/api/v1/can-sign")
def can_sign(req: CanSignRequest):
    return {"can_sign": True}

@app.post("/api/v1/is-authorized")
def is_authorized(req: ApiSignRequest):
    return {"authorized": True}

@app.post("/api/v1/sign")
def sign(req: ApiSignRequest):
    return {"signature": "deadbeef1234567890abcdef"}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app)
