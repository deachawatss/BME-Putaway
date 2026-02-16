# Putaway Bin Transfer

> Putaway Bin Transfer Application for Newly Weds Foods (Thailand)

## Overview

Standalone Putaway Bin Transfer application, separated from the monolithic BME-Putaway-Bulk system for independent deployment and scaling.

## Technology Stack

- **Frontend**: Angular 20 + TypeScript + Tailwind CSS
- **Backend**: Rust + Axum framework + Tiberius SQL Server driver
- **Database**: Microsoft SQL Server (BME882024)
- **Authentication**: LDAP/Active Directory + SQL fallback

## Quick Start

### Development

```bash
# Terminal 1 - Backend (Port 4402)
cd backend
cargo run

# Terminal 2 - Frontend (Port 4202)
cd frontend
npm install
npm start
```

Access: http://localhost:4202

### Production Build

```bash
# Backend
cd backend && cargo build --release

# Frontend
cd frontend && npm run build:prod
```

## Configuration

### Backend (.env)

```bash
cp backend/.env.example backend/.env
```

Key settings:
- `SERVER_PORT=4402`
- `CORS_ORIGINS=http://localhost:4202`
- `DATABASE_NAME=BME882024`
- `JWT_SECRET` (generate with `openssl rand -base64 64`)

### Frontend

API URL is configured in `frontend/src/environments/environment.ts`:
- Default: `http://localhost:4402/api`

## API Endpoints

```
GET  /api/health
GET  /api/auth/health
POST /api/auth/login
GET  /api/auth/status
GET  /api/database/status
GET  /api/putaway/lots/search
GET  /api/putaway/bins/search
POST /api/putaway/transfer
GET  /api/putaway/lot/{lot_no}
GET  /api/putaway/bin/{location}/{bin_no}
GET  /api/putaway/transactions/{lot_no}/{bin_no}
POST /api/putaway/transfer/committed
GET  /api/putaway/health
GET  /api/putaway/remarks
```

## Project Structure

```
BME-Putaway/
├── backend/
│   ├── src/
│   │   ├── main.rs              # Putaway-only routes (port 4402)
│   │   ├── handlers/
│   │   │   └── putaway.rs       # Putaway handlers
│   │   ├── services/
│   │   ├── database/
│   │   ├── models/
│   │   ├── middleware/
│   │   └── utils/
│   ├── Cargo.toml               # Package: putaway-backend
│   └── .env.example
├── frontend/
│   ├── src/app/
│   │   ├── app.routes.ts        # Putaway routes only
│   │   ├── components/
│   │   │   ├── login/
│   │   │   ├── dashboard/
│   │   │   └── putaway/         # Putaway component
│   │   └── services/
│   ├── package.json             # putaway-frontend
│   └── src/environments/
└── package.json
```

## Database

Uses the same **BME882024** database as BME-Bulk-Picking but operates on different tables:

- `LotMaster` - Inventory master
- `LotTransaction` - Audit trail
- `BinTransfer` - Bin transfers
- `Mintxdh` - Financial integration

## Authentication

- LDAP/Active Directory primary authentication
- SQL Server fallback for local users
- JWT tokens with configurable duration
- **Note**: Use the same `JWT_SECRET` as BME-Bulk-Picking for shared authentication

## Scripts

```bash
npm run dev:backend    # Start backend
npm run dev:frontend   # Start frontend
npm run dev:all        # (Run in separate terminals)
npm run build:backend  # Build release binary
npm run build:frontend # Build production bundle
npm run build:all      # Build both
npm run test:backend   # Run Rust tests
npm run test:frontend  # Run Angular tests
```

## Deployment

### Same Server (Different Ports)
```
Backend:  http://server:4402
Frontend: http://server:4202
```

### Separate Servers
```
Putaway Server: http://putaway-server:4400
```

## Integration with BME-Bulk-Picking

- Same database (BME882024)
- Can share JWT_SECRET for SSO
- Independent deployment
- No shared code dependencies

## License

Copyright © 2025 Newly Weds Foods (Thailand). All rights reserved.
